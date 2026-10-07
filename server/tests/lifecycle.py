"""Real HTTP integration checks; no external server, dataset or cloud account required."""

import concurrent.futures
import http.client
import json
import os
from pathlib import Path
import signal
import socket
import sqlite3
import struct
import subprocess
import tempfile
import threading
import time
import unittest
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

BINARY = Path(os.environ.get("AEGISX_BIN", Path(__file__).parents[1] / "target/debug/aegisx")).resolve()


def free_port ():

    with socket.socket() as listener:
        listener.bind(("127.0.0.1", 0))
        return listener.getsockname()[1]


def until ( check, seconds=5 ):

    end = time.monotonic() + seconds
    while time.monotonic() < end:
        value = check()
        if value: return value
        time.sleep(0.02)
    raise AssertionError("Condition did not become true before deadline")


class Backend ( BaseHTTPRequestHandler ):

    protocol_version = "HTTP/1.1"
    received = []
    lock = threading.Lock()

    def log_message ( self, *_args ): pass

    def handle_request ( self ):

        try:
            if self.headers.get("Transfer-Encoding") == "chunked":
                body = bytearray()
                while True:
                    line = self.rfile.readline().strip()
                    if not line: return
                    size = int(line, 16)
                    if not size:
                        self.rfile.readline()
                        break
                    body.extend(self.rfile.read(size))
                    self.rfile.read(2)
                body = bytes(body)
            else:
                body = self.rfile.read(int(self.headers.get("Content-Length", 0)))
            with self.lock:
                self.received.append({"id": self.headers.get("X-Request-ID"), "body": body, "headers": dict(self.headers)})
            if self.path.startswith("/timeout"): time.sleep(0.4)
            slow = self.path.startswith("/slow")
            payload = b"firstlast" if slow else body or b"ok"
            self.send_response(200)
            self.send_header("Content-Length", str(len(payload)))
            self.send_header("X-Request-ID", "untrusted-upstream-id")
            self.end_headers()
            if self.command == "HEAD": return
            if slow:
                self.wfile.write(payload[:5])
                self.wfile.flush()
                time.sleep(0.7)
                self.wfile.write(payload[5:])
            else:
                self.wfile.write(payload)
        except (BrokenPipeError, ConnectionResetError):
            pass

    do_GET = handle_request
    do_POST = handle_request
    do_HEAD = handle_request


class Runtime:

    def __init__ ( self, upstream, model=True, store=True, limits="", extra="" ):

        self.temp = tempfile.TemporaryDirectory(prefix="aegisx-test-")
        self.root = Path(self.temp.name)
        self.port = free_port()
        self.database = self.root / "events.sqlite3"
        config = self.root / "Aegisx.lua"
        self.config = config
        config.write_text(
            f'set_listen("127.0.0.1:{self.port}")\n'
            f'set_upstream("127.0.0.1:{upstream}")\n'
            f'set_model("{ "observe" if model else "off" }")\n'
            f'set_store({json.dumps(str(self.database)) if store else "false"})\n'
            f'set_limits {{ {limits} }}\n'
            + extra
        )
        self.log = (self.root / "server.log").open("w+")
        self.process = subprocess.Popen([str(BINARY), "--config", str(config)], stdout=self.log, stderr=self.log)
        until(self.ready)

    def ready ( self ):

        if self.process.poll() is not None:
            self.log.seek(0)
            raise AssertionError(self.log.read())
        try:
            with socket.create_connection(("127.0.0.1", self.port), timeout=0.1): return True
        except OSError: return False

    def request ( self, path="/", body=None, headers=None ):

        connection = http.client.HTTPConnection("127.0.0.1", self.port, timeout=3)
        connection.request("POST" if body is not None else "GET", path, body, headers or {})
        response = connection.getresponse()
        result = (response.status, response.getheader("X-Request-ID"), response.read())
        connection.close()
        return result

    def events ( self, request_id ):

        if not self.database.is_file(): return []
        with sqlite3.connect(self.database) as database:
            return [json.loads(row[0]) for row in database.execute(
                "SELECT payload FROM events WHERE request_id = ? ORDER BY sequence", (request_id,),
            )]

    def terminal ( self, request_id ):

        events = [event for event in self.events(request_id) if event["stage"] not in {"analyzed", "analysis_skipped"}]
        return events if events and events[-1]["stage"] in {"completed", "blocked", "failed"} else None

    def close ( self ):

        if self.process.poll() is None:
            self.process.send_signal(signal.SIGTERM)
            try: self.process.wait(timeout=12)
            except subprocess.TimeoutExpired:
                self.process.kill()
                self.process.wait()
                self.log.seek(0)
                raise AssertionError("Proxy did not shut down: " + self.log.read()[-8000:])
        self.log.close()
        self.temp.cleanup()

    def __enter__ ( self ): return self

    def __exit__ ( self, *_args ): self.close()


class LifecycleTests ( unittest.TestCase ):

    @classmethod
    def setUpClass ( cls ):

        cls.backend = ThreadingHTTPServer(("127.0.0.1", 0), Backend)
        cls.worker = threading.Thread(target=cls.backend.serve_forever, daemon=True)
        cls.worker.start()
        cls.upstream = cls.backend.server_port

    @classmethod
    def tearDownClass ( cls ):

        cls.backend.shutdown()
        cls.backend.server_close()
        cls.worker.join()

    def test_roundtrip_identity_privacy_and_history ( self ):

        with Runtime(self.upstream) as proxy:
            status, request_id, body = proxy.request(
                "/private/SECRET?token=SECRET", b"PRIVATE-BODY",
                {"Authorization": "Bearer SECRET", "Cookie": "SECRET", "X-Request-ID": "forged",
                 "X-Forwarded-For": "1.2.3.4", "Forwarded": "for=1.2.3.4"},
            )
            self.assertEqual((status, body), (200, b"PRIVATE-BODY"))
            self.assertNotIn(request_id, {"forged", "untrusted-upstream-id", None})
            events = until(lambda: proxy.terminal(request_id))
            self.assertEqual(events[0]["stage"], "received")
            self.assertEqual(events[-1]["stage"], "completed")
            self.assertEqual(events[-1]["details"]["response_bytes"], len(body))
            self.assertEqual([e["sequence"] for e in events], list(range(1, len(events) + 1)))
            self.assertNotIn("SECRET", json.dumps(events))
            self.assertNotIn("PRIVATE-BODY", json.dumps(events))
            with Backend.lock: backend = next(row for row in Backend.received if row["id"] == request_id)
            self.assertEqual(backend["headers"]["x-forwarded-for"], "127.0.0.1")
            self.assertNotIn("forwarded", {name.lower() for name in backend["headers"]})
            _, second, _ = proxy.request()
            events = until(lambda: proxy.terminal(second))
            decision = next(e["details"] for e in events if e["stage"] == "inspected")
            self.assertGreater(decision["features"][7], 0)
            self.assertEqual(decision["model_state"], "deferred")
            inspected = subprocess.check_output([str(BINARY), "inspect", "--database", str(proxy.database), "--request", second])
            self.assertGreaterEqual(len(json.loads(inspected)), len(events))

    def test_stream_is_live_before_completion ( self ):

        with Runtime(self.upstream, model=False) as proxy:
            connection = http.client.HTTPConnection("127.0.0.1", proxy.port, timeout=3)
            started = time.monotonic()
            connection.request("GET", "/slow")
            response = connection.getresponse()
            request_id = response.getheader("X-Request-ID")
            self.assertEqual(response.read(5), b"first")
            self.assertLess(time.monotonic() - started, 0.5)
            self.assertEqual(proxy.events(request_id), [])  # One atomic journey batch at completion.
            self.assertIsNone(proxy.terminal(request_id))
            self.assertEqual(response.read(), b"last")
            self.assertEqual(until(lambda: proxy.terminal(request_id))[-1]["stage"], "completed")
            connection.close()

    def test_explicit_buffering_preserves_complete_stream ( self ):
        # Nonzero buffering explicitly trades first-chunk latency for fewer writes.
        # The separate default-stream test requires prompt delivery.
        with Runtime(self.upstream, model=False, extra="set_runtime {write_buffer_bytes=4096}") as proxy:
            connection = http.client.HTTPConnection("127.0.0.1", proxy.port, timeout=3)
            try:
                connection.request("GET", "/slow")
                response = connection.getresponse()
                self.assertEqual((response.status, response.read()), (200, b"firstlast"))
            finally:
                connection.close()

    def test_rate_limit_prevents_forwarding ( self ):

        with Runtime(self.upstream, model=False, limits="rate_limit_10s = 1") as proxy:
            self.assertEqual(proxy.request()[0], 200)
            status, request_id, _ = proxy.request()
            self.assertEqual(status, 429)
            events = until(lambda: proxy.terminal(request_id))
            self.assertEqual(events[-1]["stage"], "blocked")
            self.assertFalse(any(e["stage"] == "upstream_selected" for e in events))

    def test_upstream_failure_and_timeout ( self ):

        with Runtime(free_port(), model=False) as proxy:
            status, request_id, _ = proxy.request()
            self.assertEqual(status, 502)
            self.assertEqual(until(lambda: proxy.terminal(request_id))[-1]["stage"], "failed")
        with Runtime(self.upstream, model=False, limits="timeout_ms = 100") as proxy:
            status, request_id, _ = proxy.request("/timeout")
            self.assertEqual(status, 504)
            self.assertEqual(until(lambda: proxy.terminal(request_id))[-1]["stage"], "failed")

    def test_declared_body_limit ( self ):

        with Runtime(self.upstream, model=False, limits="max_body_bytes = 4") as proxy:
            status, request_id, _ = proxy.request(body=b"12345")
            self.assertEqual(status, 413)
            events = until(lambda: proxy.terminal(request_id))
            self.assertFalse(any(e["stage"] == "upstream_selected" for e in events))

    def test_proxy_only_and_concurrency ( self ):

        with Runtime(self.upstream, model=False, store=False) as proxy:
            with concurrent.futures.ThreadPoolExecutor(max_workers=8) as pool:
                results = list(pool.map(lambda _: proxy.request(), range(24)))
            self.assertTrue(all(row[0] == 200 for row in results))
            self.assertEqual(len({row[1] for row in results}), 24)
            self.assertFalse(proxy.database.exists())
            threads = [path.read_text().strip() for path in Path(f"/proc/{proxy.process.pid}/task").glob("*/comm")]
            self.assertNotIn("aegisx-inferenc", threads)
            self.assertNotIn("aegisx-store", threads)


    def test_keepalive_uses_distinct_request_ids ( self ):

        with Runtime(self.upstream, model=False) as proxy:
            connection = http.client.HTTPConnection("127.0.0.1", proxy.port, timeout=3)
            ids = []
            for _ in range(2):
                connection.request("GET", "/")
                response = connection.getresponse()
                self.assertEqual(response.status, 200)
                ids.append(response.getheader("X-Request-ID"))
                response.read()
            connection.close()
            self.assertNotEqual(*ids)
            for request_id in ids:
                self.assertEqual(until(lambda: proxy.terminal(request_id))[-1]["stage"], "completed")

    def test_chunked_upload_and_limit ( self ):

        with Runtime(self.upstream, model=False, limits="max_body_bytes = 4") as proxy:
            connection = http.client.HTTPConnection("127.0.0.1", proxy.port, timeout=3)
            connection.request("POST", "/", iter([b"12", b"34"]), encode_chunked=True)
            response = connection.getresponse()
            self.assertEqual((response.status, response.read()), (200, b"1234"))
            connection.close()
            connection = http.client.HTTPConnection("127.0.0.1", proxy.port, timeout=3)
            connection.request("POST", "/", iter([b"12345"]), encode_chunked=True)
            response = connection.getresponse()
            request_id = response.getheader("X-Request-ID")
            self.assertEqual(response.status, 413)
            response.read()
            connection.close()
            events = until(lambda: proxy.terminal(request_id))
            self.assertTrue(any(e["stage"] == "rejected" for e in events))
            self.assertNotEqual(events[-1]["stage"], "completed")

    def test_disconnect_is_not_reported_as_completed ( self ):

        with Runtime(self.upstream, model=False) as proxy:
            stream = socket.create_connection(("127.0.0.1", proxy.port), timeout=3)
            stream.sendall(b"GET /slow HTTP/1.1\r\nHost: localhost\r\n\r\n")
            header = b""
            while b"\r\n\r\n" not in header: header += stream.recv(4096)
            request_id = next(line.split(b":", 1)[1].strip().decode() for line in header.split(b"\r\n") if line.lower().startswith(b"x-request-id:"))
            stream.setsockopt(socket.SOL_SOCKET, socket.SO_LINGER, struct.pack("ii", 1, 0))
            stream.close()
            events = until(lambda: proxy.terminal(request_id))
            self.assertEqual(events[-1]["stage"], "failed")
            self.assertEqual(events[-1]["details"]["status"], 200)

    def test_shutdown_preserves_active_request ( self ):

        with Runtime(self.upstream, model=False) as proxy:
            connection = http.client.HTTPConnection("127.0.0.1", proxy.port, timeout=3)
            connection.request("GET", "/slow")
            response = connection.getresponse()
            request_id = response.getheader("X-Request-ID")
            self.assertEqual(response.read(5), b"first")
            proxy.process.send_signal(signal.SIGTERM)
            self.assertEqual(response.read(), b"last")
            connection.close()
            self.assertEqual(proxy.process.wait(timeout=10), 0)
            self.assertEqual(proxy.terminal(request_id)[-1]["stage"], "completed")

    def test_lua_rejects_invalid_and_unbounded_configuration ( self ):

        with tempfile.TemporaryDirectory(prefix="aegisx-config-") as directory:
            config = Path(directory) / "test.lua"
            for text in [
                "while true do end", 'set_model("enforce")',
                "set_limits { typo = 1 }",
                'set_headers("response", {["x-test"]="a",["X-Test"]="b"})',
                "set_queue {capacity=8193}", "set_queue {timeout_ms=0}",
                "set_queue {timeout_ms=30001}", "set_queue {unknown=true}",
                "set_model {response_scan_bytes=1025}", "set_model {journey=42}",
                'set_model("background") set_cache { decisions=true, background_denials=true }',
                'set_context { shards=0 }', 'set_cache { deny_ttl_ms=0 }',
                'set_identity { actor_header="x-actor" }',
                'set_identity { request_id_header="authorization" }', "os.execute('true')",
                "pcall(function() while true do end end)",
                'set_listen("127.0.0.1:3000")',
            ]:
                config.write_text(text)
                result = subprocess.run([str(BINARY), "--config", str(config), "--check"], capture_output=True, timeout=3)
                self.assertNotEqual(result.returncode, 0, text)


if __name__ == "__main__":
    unittest.main(verbosity=2)
