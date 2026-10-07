"""Real streaming response capture, instrumented spans, opt-outs and privacy."""
import gzip
import http.client
import json
import os
from pathlib import Path
import sqlite3
import threading
import time
import unittest
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from unittest.mock import patch

from integration_support import Fixture
from lifecycle import until

BACKEND_TOKEN = "aegisx-lifecycle-backend-token-00000000"
PAYLOAD = b'{"message":"PRIVATE-RESPONSE-TEXT","ok":true}'


class TextJourneyTests ( Fixture, unittest.TestCase ):
    def start_instrumented_origin ( self, binary=False, compressed=False, stream=False ):
        fixture = self

        class Handler ( BaseHTTPRequestHandler ):
            protocol_version = "HTTP/1.1"
            def log_message ( self, *_ ): pass
            def do_POST ( self ):
                self.rfile.read(int(self.headers.get("Content-Length", "0")))
                request_id = self.headers["X-Request-ID"]
                events = [
                    {"service": "api", "operation": "request", "state": "started", "span_id": "root", "elapsed_ms": 1},
                    {"service": "store", "operation": "write", "state": "completed", "span_id": "db", "parent_id": "root", "duration_ms": 2, "elapsed_ms": 3}]
                for event in events:
                    code, _ = fixture.api("/backend/events", event | {"request_id": request_id}, token=BACKEND_TOKEN)
                    fixture.assertEqual(code, 202)
                code, _ = fixture.api("/backend/events", events[1] | {"request_id": request_id, "parent_id": "db"}, token=BACKEND_TOKEN)
                fixture.assertEqual(code, 400)
                body = gzip.compress(PAYLOAD) if compressed else PAYLOAD
                self.send_response(200)
                self.send_header("Content-Type", "application/octet-stream" if binary else "application/json")
                if compressed: self.send_header("Content-Encoding", "gzip")
                self.send_header("Content-Length", str(len(body)))
                self.end_headers()
                if stream:
                    self.wfile.write(body[:5])
                    self.wfile.flush()
                    time.sleep(.4)
                    self.wfile.write(body[5:])
                else: self.wfile.write(body)

        origin = ThreadingHTTPServer(("127.0.0.1", 0), Handler)
        worker = threading.Thread(target=origin.serve_forever, daemon=True)
        worker.start()
        return origin, worker

    def exchange ( self, *, budget=1024, journey=True, binary=False, compressed=False, stream=False ):
        origin, worker = self.start_instrumented_origin(binary, compressed, stream)
        prior, self.upstream = self.upstream, origin.server_port
        try:
            extra = f'set_model {{mode="observe",response_scan_bytes={budget},journey={str(journey).lower()}}}'
            with patch.dict(os.environ, {"AEGISX_BACKEND_TOKEN": BACKEND_TOKEN}), self.proxy(extra, control=True, backend=True, store=True) as proxy:
                connection = http.client.HTTPConnection("127.0.0.1", proxy.port, timeout=3)
                connection.request("POST", "/api/records", b"PRIVATE-REQUEST-TEXT", {"Content-Type": "application/json"})
                response = connection.getresponse()
                request_id = response.getheader("X-Request-ID")
                self.assertEqual(response.status, 200)
                started = time.monotonic()
                prefix = response.read(5)
                if stream:
                    self.assertLess(time.monotonic() - started, .3)
                    self.assertEqual(self.api("/state")[1]["analysis"]["submitted"], 0)
                body = prefix + response.read()
                connection.close()
                self.assertEqual(gzip.decompress(body) if compressed else body, PAYLOAD)
                def analysis ():
                    return next((event for event in proxy.events(request_id) if event["stage"] == "analyzed"), None)
                event = until(analysis)
                details = event["details"]
                coverage = details["model_inputs"]
                self.assertEqual(coverage["backend_events"], 2 if journey else 0)
                self.assertEqual(coverage["request_bytes"], len(b"POST /api/records\nPRIVATE-REQUEST-TEXT"))
                captured = budget > 0 and not binary and not compressed
                self.assertEqual(coverage["response_available"], captured)
                self.assertEqual(coverage["response_bytes"], min(budget, len(PAYLOAD)) if captured else 0)
                self.assertEqual(coverage["response_truncated"], captured and budget < len(PAYLOAD))
                self.assertEqual(details["decision"], "completed_request_unchanged")
                self.assertEqual(set(details["component_scores"]), {"risk", "content", "journey"})
                if not journey: self.assertEqual(details["component_scores"]["journey"], 0)
                self.assertEqual(self.api("/state")[1]["analysis"]["finished"], 1)
                with sqlite3.connect(proxy.database) as database:
                    stored = "\n".join(database.iterdump())
                self.assertNotIn("PRIVATE-REQUEST-TEXT", stored)
                self.assertNotIn("PRIVATE-RESPONSE-TEXT", stored)
                self.assertNotIn(BACKEND_TOKEN, stored)
        finally:
            self.upstream = prior
            origin.shutdown()
            origin.server_close()
            worker.join()

    def test_legacy_numeric_artifact_uses_its_actual_input_contract ( self ):
        directory = Path(__file__).parent / "fixtures/numeric-v4"
        self.assertTrue(directory.is_dir(), "The numeric-v4 compatibility fixture is required")
        extra = 'set_model {mode="observe",directory=' + json.dumps(str(directory)) + '}'
        with self.proxy(extra, control=True, store=True) as proxy:
            status, headers, _ = self.fetch(proxy.port)
            self.assertEqual(status, 200)
            request_id = self.header(headers, "x-request-id")
            event = until(lambda: next((item for item in proxy.events(request_id) if item["stage"] == "analyzed"), None))
            self.assertEqual(event["details"]["model_inputs"]["schema"], "numeric-v4")
            self.assertEqual(event["details"]["component_scores"]["journey"], 0)
            self.assertIsNotNone(event["details"]["risk_score"])

    def test_completed_stream_is_analyzed_with_request_response_and_parent_spans ( self ):
        self.exchange(stream=True)

    def test_explicit_small_response_budget_reports_truncation ( self ):
        self.exchange(budget=8)

    def test_capture_opt_out_and_journey_opt_out ( self ):
        self.exchange(budget=0, journey=False)

    def test_binary_and_compressed_content_is_not_misrepresented_as_text ( self ):
        for values in ({"binary": True}, {"compressed": True}):
            with self.subTest(**values): self.exchange(**values)


if __name__ == "__main__": unittest.main(verbosity=2)
