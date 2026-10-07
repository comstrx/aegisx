"""Shared local fixtures for cache, control and backend contract tests."""

import http.client
import json
import threading
from collections import Counter
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from unittest.mock import patch

from lifecycle import Runtime, free_port, until

TOKEN = "aegisx-integration-test-token-000000000"
SECRET = "aegisx-integration-test-signing-secret-000000"


class Origin ( BaseHTTPRequestHandler ):

    protocol_version = "HTTP/1.1"
    health_status = 200
    counts = Counter()
    requests = []
    hooks = []
    lock = threading.Lock()

    def log_message ( self, *_args ): pass

    def do_GET ( self ):

        with self.lock:
            self.counts[self.path] += 1
            number = self.counts[self.path]
            self.requests.append((self.path, dict(self.headers)))
        body = json.dumps({"path": self.path, "number": number}).encode()
        if self.path.startswith("/large"): body = b"x" * 5000
        self.send_response(self.health_status if self.path == "/health" else 200)
        self.send_header("Content-Length", str(len(body)))
        policy = "public, max-age=60"
        if self.path.startswith("/private"): policy = "private, max-age=60"
        if self.path.startswith("/no-store"): policy = "public, no-store, max-age=60"
        self.send_header("Cache-Control", policy)
        if self.path.startswith("/cookie"): self.send_header("Set-Cookie", "session=test")
        if self.path.startswith("/vary"): self.send_header("Vary", "X-Customer")
        if self.path.startswith("/signal"): self.send_header("X-Backend-Ban", "99999")
        self.send_header("X-Request-ID", "spoofed-origin")
        self.end_headers()
        self.wfile.write(body)

    def do_POST ( self ):

        body = self.rfile.read(int(self.headers.get("Content-Length", 0)))
        with self.lock:
            self.hooks.append((dict(self.headers), body))
            number = len(self.hooks)
        status = 500 if self.path == "/retry" and number == 1 else 204
        if self.path == "/permanent": status = 400
        self.send_response(status)
        self.send_header("Content-Length", "0")
        self.end_headers()


class Fixture:

    @classmethod
    def setUpClass ( cls ):

        cls.origin = ThreadingHTTPServer(("127.0.0.1", 0), Origin)
        cls.worker = threading.Thread(target=cls.origin.serve_forever, daemon=True)
        cls.worker.start()
        cls.upstream = cls.origin.server_port

    @classmethod
    def tearDownClass ( cls ):

        cls.origin.shutdown()
        cls.origin.server_close()
        cls.worker.join()

    def setUp ( self ):

        with Origin.lock:
            Origin.counts.clear()
            Origin.requests.clear()
            Origin.hooks.clear()

    def proxy ( self, extra="", control=False, model=False, store=False, backend=False ):

        self.admin = free_port()
        if control:
            integration = ', backend_token_env="AEGISX_BACKEND_TOKEN"' if backend else ''
            extra += f'\nset_control {{ enabled=true, listen="127.0.0.1:{self.admin}", api_prefix="/manage/v1"{integration} }}\n'
        with patch.dict("os.environ", {"AEGISX_ADMIN_TOKEN": TOKEN, "AEGISX_TEST_HOOK_KEY": SECRET}):
            proxy = Runtime(self.upstream, model=model, store=store or "decisions=true" in extra, extra=extra)
        if control: until(lambda: self.api("/state")[0] == 200)
        return proxy

    def api ( self, path, data=None, token=TOKEN, headers=None ):

        values = {"Authorization": f"Bearer {token}"}
        values.update(headers or {})
        status, response_headers, body = self.fetch(self.admin, "/manage/v1" + path,
            "POST" if data is not None else "GET", None if data is None else json.dumps(data), values)
        return status, json.loads(body) if body else None

    @staticmethod
    def fetch ( port, path="/", method="GET", body=None, headers=None ):

        connection = http.client.HTTPConnection("127.0.0.1", port, timeout=3)
        connection.request(method, path, body, headers or {})
        response = connection.getresponse()
        result = response.status, dict(response.getheaders()), response.read()
        connection.close()
        return result

    @staticmethod
    def header ( headers, name ):

        return next((value for key, value in headers.items() if key.lower() == name.lower()), None)
