"""Real admission pressure, SQLite recovery and interrupted-analysis contracts."""
import concurrent.futures
import json
import os
import signal
import sqlite3
import subprocess
import threading
import unittest
import uuid
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

from integration_support import Fixture, TOKEN, Origin
from lifecycle import BINARY, until


class HeldOrigin(BaseHTTPRequestHandler):
    protocol_version = "HTTP/1.1"
    release = threading.Event()
    entered = 0
    lock = threading.Lock()
    def log_message(self, *_): pass
    def do_GET(self):
        with self.lock: type(self).entered += 1
        self.release.wait(8)
        self.send_response(200)
        self.send_header("Content-Length", "2")
        self.end_headers()
        self.wfile.write(b"ok")


class PressureTests(Fixture, unittest.TestCase):

    def test_storage_reservation_rejects_before_forwarding_and_preserves_accepted_journeys(self):
        self.held_capacity(model=False, capacity=16, count=16)

    def test_model_reservation_includes_inflight_requests_and_never_drops_admitted_job(self):
        self.held_capacity(model=True, capacity=1, count=1)

    def held_capacity(self, model, capacity, count):
        HeldOrigin.release.clear()
        HeldOrigin.entered=0
        origin=ThreadingHTTPServer(("127.0.0.1",0),HeldOrigin)
        origin.request_queue_size=64
        thread=threading.Thread(target=origin.serve_forever,daemon=True);thread.start()
        previous=self.upstream;self.upstream=origin.server_port
        try:
            extra='set_limits {queue_capacity=16}\nset_runtime {max_in_flight=64}\n'
            if model: extra+=f'set_model {{mode="observe",queue_capacity={capacity},on_overload="reject"}}'
            with self.proxy(extra,control=True,model=model,store=True) as proxy:
                with concurrent.futures.ThreadPoolExecutor(max_workers=count) as pool:
                    work=[pool.submit(self.fetch,proxy.port) for _ in range(count)]
                    until(lambda:HeldOrigin.entered==count)
                    state=self.api("/state")[1]
                    self.assertEqual(state["journeys"]["total"],count)
                    self.assertEqual(self.fetch(proxy.port)[0],503)
                    self.assertEqual(HeldOrigin.entered,count)
                    HeldOrigin.release.set()
                    results=[item.result(timeout=4) for item in work]
                self.assertTrue(all(status==200 for status,_,_ in results))
                for _,headers,_ in results:
                    request=self.header(headers,"x-request-id")
                    events=until(lambda:proxy.terminal(request))
                    self.assertEqual(events[0]["stage"],"received")
                    self.assertEqual(events[-1]["stage"],"completed")
                state=self.api("/state")[1]
                self.assertEqual(state["storage"]["dropped_batches"],0)
                if model:
                    self.assertEqual(state["analysis"]["dropped"],0)
                    self.assertEqual(state["analysis"]["pressure_rejections"],1)
                    until(lambda:self.api("/state")[1]["analysis"]["finished"]==1)
                else: self.assertEqual(state["storage"]["pressure_rejections"],1)
        finally:
            HeldOrigin.release.set();self.upstream=previous
            origin.shutdown();origin.server_close();thread.join()

    def test_sqlite_write_failure_retains_batch_recovers_and_closes_required_admission(self):
        with self.proxy(control=True,store=True) as proxy:
            database=sqlite3.connect(proxy.database,timeout=1)
            try:
                database.execute("BEGIN IMMEDIATE")
                status,headers,_=self.fetch(proxy.port,"/first")
                self.assertEqual(status,200)
                request=self.header(headers,"x-request-id")
                until(lambda:self.api("/state")[1]["storage"]["write_retries"]>0)
                self.assertFalse(self.api("/state")[1]["storage"]["healthy"])
                self.assertEqual(self.fetch(proxy.port,"/must-not-forward")[0],503)
                self.assertEqual(Origin.counts["/must-not-forward"],0)
                database.rollback()
                self.assertEqual(until(lambda:proxy.terminal(request))[-1]["stage"],"completed")
                until(lambda:self.api("/state")[1]["storage"]["healthy"])
                self.assertEqual(self.fetch(proxy.port,"/recovered")[0],200)
                self.assertEqual(self.api("/state")[1]["storage"]["dropped_batches"],0)
            finally: database.close()

    def test_retention_never_leaves_partial_committed_journeys(self):
        with self.proxy('set_limits {retention_events=100}',control=True,store=True) as proxy:
            results=[self.fetch(proxy.port) for _ in range(220)]
            self.assertTrue(all(item[0]==200 for item in results))
            until(lambda:self.api("/state")[1]["storage"]["committed_batches"]==220)
            with sqlite3.connect(proxy.database) as database:
                counts=database.execute("SELECT COUNT(*) FROM events GROUP BY request_id").fetchall()
                oldest=database.execute("SELECT COUNT(*) FROM events WHERE request_id=?",(self.header(results[0][1],"x-request-id"),)).fetchone()[0]
            self.assertEqual(oldest,0)
            self.assertGreater(len(counts),0)
            self.assertEqual(len({row[0] for row in counts}),1)
            self.assertGreater(counts[0][0],3)

    def test_permanent_storage_failure_reports_failed_shutdown_without_hanging(self):
        with self.proxy(control=True,store=True) as proxy:
            database=sqlite3.connect(proxy.database,timeout=1)
            try:
                database.execute("BEGIN IMMEDIATE")
                self.assertEqual(self.fetch(proxy.port)[0],200)
                until(lambda:self.api("/state")[1]["storage"]["write_retries"]>0)
                proxy.process.send_signal(signal.SIGTERM)
                self.assertNotEqual(proxy.process.wait(timeout=8),0)
            finally: database.close()

    def test_committed_pending_job_recovers_after_kill_without_replaying_enforcement(self):
        extra='set_model {mode="background",threshold=0,allow_unvalidated=true}\nset_cache {decisions=true,background_denials=true}'
        with self.proxy(extra,control=True,store=True) as proxy:
            state=self.api("/state")[1]
            artifact=state["model"]["artifact_sha256"]
            with sqlite3.connect(proxy.database) as database:
                for digest,features in [(artifact,[0.0]*296),("different-artifact",[0.0]*296),(artifact,[float("inf")]*296)]:
                    database.execute("INSERT INTO analysis_jobs(request_id,artifact,created_ms,features,state) VALUES (?,?,?,?,?)",
                        (str(uuid.uuid4()),digest,0,json.dumps(features),"pending"))
            proxy.process.kill()
            proxy.process.wait(timeout=5)
            env=dict(os.environ,AEGISX_ADMIN_TOKEN=TOKEN)
            proxy.process=subprocess.Popen([str(BINARY),"--config",str(proxy.config)],stdout=proxy.log,stderr=proxy.log,env=env)
            until(proxy.ready)
            until(lambda:self.api("/state")[0]==200)
            state=self.api("/state")[1]
            self.assertEqual(state["analysis"]["recovered"],3)
            self.assertEqual(self.api("/decisions")[1]["items"],[])
            with sqlite3.connect(proxy.database) as database:
                rows=database.execute("SELECT state,score FROM analysis_jobs ORDER BY id").fetchall()
            self.assertEqual(rows, [("recovery_unavailable",None)]*3)
            self.assertEqual(self.fetch(proxy.port)[0],200)


if __name__=="__main__": unittest.main(verbosity=2)
