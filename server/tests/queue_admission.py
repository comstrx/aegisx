"""Live bounded waiting, timeout and decision-coherence tests on loopback."""
import threading
import unittest
from concurrent.futures import ThreadPoolExecutor
from http.server import ThreadingHTTPServer

from integration_support import Fixture, Origin
from lifecycle import until


class HeldOrigin(Origin):
    entered = threading.Event()
    release = threading.Event()
    held = 0

    def do_GET(self):
        if self.path == "/hold":
            with self.lock: type(self).held += 1
            self.entered.set()
            self.release.wait(3)
        super().do_GET()


class QueueOrigin(ThreadingHTTPServer):
    request_queue_size = 64


class QueueTests(Fixture, unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.origin = QueueOrigin(("127.0.0.1", 0), HeldOrigin)
        cls.worker = threading.Thread(target=cls.origin.serve_forever, daemon=True)
        cls.worker.start()
        cls.upstream = cls.origin.server_port

    def setUp(self):
        super().setUp()
        HeldOrigin.held = 0
        HeldOrigin.entered.clear()
        HeldOrigin.release.clear()

    def queued(self):
        return self.api("/state")[1]["queue"]

    def test_busy_global_capacity_waits_and_forwards_every_admitted_request(self):
        extra = 'set_runtime {threads=4,max_in_flight=1}\nset_queue {capacity=4,timeout_ms=1500}'
        with self.proxy(extra, control=True) as proxy, ThreadPoolExecutor(max_workers=4) as clients:
            first = clients.submit(self.fetch, proxy.port, "/hold")
            self.assertTrue(HeldOrigin.entered.wait(1))
            later = [clients.submit(self.fetch, proxy.port, "/later") for _ in range(3)]
            try:
                until(lambda: self.queued()["waiting"] == 3)
                self.assertEqual(Origin.counts["/later"], 0)
                active = self.api("/state")[1]["journeys"]["items"]
                self.assertTrue(any(any(event["stage"] == "queued" for event in row["events"]) for row in active))
            finally:
                HeldOrigin.release.set()
            self.assertEqual(first.result()[0], 200)
            self.assertEqual([future.result()[0] for future in later], [200] * 3)
            self.assertEqual(Origin.counts["/later"], 3)
            until(lambda: self.queued()["waiting"] == 0)
            self.assertEqual(self.queued()["resumed"], 3)

    def test_queue_full_and_timeout_do_not_forward_or_grow_without_limit(self):
        extra = 'set_runtime {max_in_flight=1}\nset_queue {capacity=1,timeout_ms=250}'
        with self.proxy(extra, control=True) as proxy, ThreadPoolExecutor(max_workers=2) as clients:
            first = clients.submit(self.fetch, proxy.port, "/hold")
            self.assertTrue(HeldOrigin.entered.wait(1))
            waiting = clients.submit(self.fetch, proxy.port, "/expires")
            try:
                until(lambda: self.queued()["waiting"] == 1)
                self.assertEqual(self.fetch(proxy.port, "/overflow")[0], 503)
                self.assertEqual(waiting.result()[0], 503)
                self.assertEqual(Origin.counts["/expires"] + Origin.counts["/overflow"], 0)
                self.assertEqual(self.queued()["full"], 1)
                self.assertEqual(self.queued()["timed_out"], 1)
            finally:
                HeldOrigin.release.set()
            self.assertEqual(first.result()[0], 200)

    def test_upstream_capacity_resumes_on_release(self):
        extra = f'''set_queue {{capacity=4,timeout_ms=1500}}
add_upstream("limited", {{address="127.0.0.1:{self.upstream}",max_in_flight=1}})
set_default_upstream("limited")
'''
        with self.proxy(extra, control=True) as proxy, ThreadPoolExecutor(max_workers=2) as clients:
            first = clients.submit(self.fetch, proxy.port, "/hold")
            self.assertTrue(HeldOrigin.entered.wait(1))
            waiting = clients.submit(self.fetch, proxy.port, "/later")
            try:
                until(lambda: self.queued()["waiting"] == 1)
                self.assertEqual(Origin.counts["/later"], 0)
            finally:
                HeldOrigin.release.set()
            self.assertEqual(first.result()[0], 200)
            self.assertEqual(waiting.result()[0], 200)
            self.assertEqual(Origin.counts["/later"], 1)
            self.assertEqual(self.queued()["resumed"], 1)

    def test_waiting_upload_reaches_backend_once_with_original_body_and_id(self):
        extra = 'set_runtime {max_in_flight=1}\nset_queue {capacity=2,timeout_ms=1500}'
        payload = b"queued-body-data-" * 16384
        with self.proxy(extra, control=True) as proxy, ThreadPoolExecutor(max_workers=2) as clients:
            first = clients.submit(self.fetch, proxy.port, "/hold")
            self.assertTrue(HeldOrigin.entered.wait(1))
            waiting = clients.submit(self.fetch, proxy.port, "/upload", "POST", payload)
            try:
                until(lambda: self.queued()["waiting"] == 1)
                self.assertEqual(Origin.hooks, [])
            finally:
                HeldOrigin.release.set()
            self.assertEqual(first.result()[0], 200)
            status, headers, _ = waiting.result()
            self.assertEqual(status, 204)
            self.assertEqual(len(Origin.hooks), 1)
            backend_headers, body = Origin.hooks[0]
            self.assertEqual(body, payload)
            self.assertEqual(self.header(backend_headers, "x-request-id"), self.header(headers, "x-request-id"))

    def test_analysis_queue_wait_preserves_completed_job(self):
        self.wait_for_completion_capacity(model=True)

    def test_storage_queue_wait_preserves_full_journey(self):
        self.wait_for_completion_capacity(model=False)

    def wait_for_completion_capacity(self, model):
        count = 1 if model else 16
        extra = 'set_queue {capacity=32,timeout_ms=1500}\nset_limits {queue_capacity=16}\nset_runtime {threads=4,max_in_flight=64}'
        if model: extra += '\nset_model {mode="observe",queue_capacity=1,on_overload="reject"}'
        with self.proxy(extra, control=True, store=True, model=model) as proxy, ThreadPoolExecutor(max_workers=count + 1) as clients:
            initial = [clients.submit(self.fetch, proxy.port, "/hold") for _ in range(count)]
            until(lambda: HeldOrigin.held == count)
            waiting = clients.submit(self.fetch, proxy.port, "/later")
            try:
                until(lambda: self.queued()["waiting"] == 1)
                self.assertEqual(Origin.counts["/later"], 0)
            finally:
                HeldOrigin.release.set()
            responses = [future.result() for future in initial] + [waiting.result()]
            self.assertTrue(all(status == 200 for status, _, _ in responses))
            for _, headers, _ in responses:
                events = until(lambda: proxy.terminal(self.header(headers, "x-request-id")))
                self.assertEqual(events[-1]["stage"], "completed")
            until(lambda: self.api("/state")[1]["storage"]["committed_batches"] == count + 1)
            state = self.api("/state")[1]
            self.assertEqual(state["storage"]["dropped_batches"], 0)
            self.assertEqual(state["storage"]["pressure_rejections"], 0)
            self.assertEqual(Origin.counts["/later"], 1)
            if model:
                until(lambda: self.api("/state")[1]["analysis"]["finished"] == count + 1)
                self.assertEqual(self.api("/state")[1]["analysis"]["dropped"], 0)

    def test_ban_committed_while_queued_is_rechecked_before_forwarding(self):
        extra = f'''set_queue {{capacity=4,timeout_ms=1500}}
set_cache {{decisions=true}}
add_upstream("limited", {{address="127.0.0.1:{self.upstream}",max_in_flight=1}})
set_default_upstream("limited")
'''
        with self.proxy(extra, control=True, store=True) as proxy, ThreadPoolExecutor(max_workers=2) as clients:
            first = clients.submit(self.fetch, proxy.port, "/hold")
            self.assertTrue(HeldOrigin.entered.wait(1))
            waiting = clients.submit(self.fetch, proxy.port, "/later")
            try:
                until(lambda: self.queued()["waiting"] == 1)
                state = self.api("/state")[1]
                actor = next(row["actor"] for row in state["journeys"]["items"] if row["actor"])
                status, _ = self.api("/blocks", {"config_version": state["config_version"],
                    "actor": actor, "route": "default", "ttl_ms": 5000, "reason": "queued-ban"})
                self.assertEqual(status, 200)
            finally:
                HeldOrigin.release.set()
            self.assertEqual(first.result()[0], 200)
            self.assertEqual(waiting.result()[0], 403)
            self.assertEqual(Origin.counts["/later"], 0)


if __name__ == "__main__":
    unittest.main(verbosity=2)
