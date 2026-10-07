"""Contract tests for post-completion inference and database-backed decisions."""
import concurrent.futures
import json
import os
import signal
import subprocess
import threading
import unittest
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from unittest.mock import patch

from integration_support import Fixture, TOKEN
from lifecycle import BINARY, until

BACKEND_TOKEN = "aegisx-backend-test-credential-000000000000"

class SlowOrigin(BaseHTTPRequestHandler):
    protocol_version = "HTTP/1.1"
    release = threading.Event()
    written = threading.Event()
    request_id = None
    writes = 0

    def log_message(self, *_args): pass
    def do_POST(self):
        self.rfile.read(int(self.headers.get("Content-Length", 0)))
        type(self).writes += 1
        type(self).request_id = self.headers.get("X-Request-ID")
        self.written.set()
        self.release.wait(5)
        self.send_response(200)
        self.send_header("Content-Length", "9")
        self.end_headers()
        self.wfile.write(b"committed")

class BackgroundSafety(Fixture, unittest.TestCase):
    def test_no_analysis_or_late_abort_before_completion_and_backend_events(self):
        SlowOrigin.release.clear()
        SlowOrigin.written.clear()
        SlowOrigin.writes = 0
        origin = ThreadingHTTPServer(("127.0.0.1", 0), SlowOrigin)
        thread = threading.Thread(target=origin.serve_forever, daemon=True)
        thread.start()
        previous = self.upstream
        self.upstream = origin.server_port
        try:
            with patch.dict(os.environ, {"AEGISX_BACKEND_TOKEN": BACKEND_TOKEN}):
                extra = """set_model { mode="background", threshold=0, allow_unvalidated=true }
set_cache { decisions=true, background_denials=true, deny_ttl_ms=10000 }
"""
                with self.proxy(extra, control=True, store=True, backend=True) as proxy:
                    with concurrent.futures.ThreadPoolExecutor() as pool:
                        result = pool.submit(self.fetch, proxy.port, "/write", "POST", "private=SECRET")
                        self.assertTrue(SlowOrigin.written.wait(3))
                        state = self.api("/state")[1]
                        self.assertEqual(state["analysis"]["submitted"], 0)
                        journey = next(item for item in state["journeys"]["items"] if item["request_id"] == SlowOrigin.request_id)
                        report = {"request_id":SlowOrigin.request_id, "service":"billing", "operation":"commit", "state":"completed", "duration_ms":12}
                        self.assertEqual(self.api("/backend/events", report, token=TOKEN)[0], 401)
                        self.assertEqual(self.api("/backend/events", report, token=BACKEND_TOKEN)[0], 202)
                        self.assertEqual(self.api("/state", token=BACKEND_TOKEN)[0], 401)
                        self.assertEqual(self.api("/backend/events", {**report,"secret":"forbidden"}, token=BACKEND_TOKEN)[0], 400)
                        action = {"config_version":state["config_version"],"route":journey["route"],"actor":journey["actor"],
                                  "ttl_ms":5000,"reason":"late operator verdict","request_id":SlowOrigin.request_id}
                        self.assertEqual(self.api("/blocks",action)[0],200)
                        SlowOrigin.release.set()
                        self.assertEqual(result.result(timeout=3)[::2],(200,b"committed"))
                        self.assertEqual(SlowOrigin.writes,1)
                    events = until(lambda: (items if any(e["stage"]=="analyzed" for e in items) else None) if (items:=proxy.events(SlowOrigin.request_id)) else None)
                    analysis = next(e for e in events if e["stage"]=="analyzed")
                    completed = next(e for e in events if e["stage"]=="completed")
                    self.assertGreater(analysis["sequence"],completed["sequence"])
                    self.assertEqual(analysis["details"]["action"],"superseded_by_operator")
                    self.assertEqual(len(analysis["details"]["features"]),296)
                    self.assertAlmostEqual(analysis["details"]["features"][32],200/599,places=6)
                    self.assertEqual(analysis["details"]["backend_events"][0]["service"],"billing")
                    self.assertNotIn("SECRET",json.dumps(events))
                    self.assertEqual(self.fetch(proxy.port,"/write","POST","again")[0],403)
        finally:
            SlowOrigin.release.set()
            self.upstream = previous
            origin.shutdown()
            origin.server_close()
            thread.join()

    def test_database_survives_cache_eviction_and_process_restart(self):
        with self.proxy('set_cache { decisions=true, deny_ttl_ms=10000 }',control=True,store=True) as proxy:
            self.fetch(proxy.port)
            state=until(lambda:self.api("/state")[1] if self.api("/state")[1]["telemetry"]["recent"] else None)
            event=state["telemetry"]["recent"][0]
            block={"config_version":state["config_version"],"route":event["details"]["route"],"actor":event["details"]["actor"],"ttl_ms":10000,"reason":"restart proof"}
            self.assertEqual(self.api("/blocks",block)[0],200)
            self.assertEqual(self.api("/cache/purge",{"kind":"decisions"})[0],200)
            self.assertEqual(self.fetch(proxy.port)[0],403)
            proxy.process.send_signal(signal.SIGTERM)
            self.assertEqual(proxy.process.wait(timeout=12),0)
            env=os.environ.copy()
            env["AEGISX_ADMIN_TOKEN"]=TOKEN
            proxy.process=subprocess.Popen([str(BINARY),"--config",str(proxy.config)],stdout=proxy.log,stderr=proxy.log,env=env)
            until(proxy.ready)
            self.assertEqual(self.fetch(proxy.port)[0],403)
            verdict=self.api("/decisions")[1]["items"][0]
            self.assertEqual(self.api("/blocks/revoke",{"key":verdict["key"]})[0],200)
            self.assertEqual(self.fetch(proxy.port)[0],200)
            self.assertGreater(self.api("/state")[1]["decisions"]["misses"],0)

if __name__=="__main__": unittest.main(verbosity=2)
