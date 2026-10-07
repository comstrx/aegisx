"""Policy integration tests against live HTTP/TLS backends."""

from collections import Counter
import http.client
import json
from pathlib import Path
import shutil
import signal
import socket
import ssl
import subprocess
import tempfile
import threading
import time
import unittest
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

from lifecycle import BINARY, Runtime, free_port, until


class Handler ( BaseHTTPRequestHandler ):

    protocol_version = "HTTP/1.1"

    def log_message ( self, *_args ): pass

    def do_GET ( self ):

        self.server.calls += 1
        body = self.rfile.read(int(self.headers.get("Content-Length", 0))).decode()
        if self.path.startswith("/drop"):
            self.connection.shutdown(socket.SHUT_RDWR)
            self.connection.close()
            return
        payload = json.dumps({"backend": self.server.name, "path": self.path, "body": body,
                              "headers": dict(self.headers)}).encode()
        self.send_response(503 if self.path.startswith("/fail") else 200)
        self.send_header("Content-Length", str(len(payload)))
        self.end_headers()
        try:
            if self.path.startswith("/slow"):
                self.wfile.write(payload[:1])
                self.wfile.flush()
                time.sleep(0.5)
                self.wfile.write(payload[1:])
            else: self.wfile.write(payload)
        except (BrokenPipeError, ConnectionResetError): pass

    do_POST = do_GET


class Server:

    def __init__ ( self, name, port=0, tls=None ):

        self.server = ThreadingHTTPServer(("127.0.0.1", port), Handler)
        self.server.name, self.server.calls = name, 0
        if tls:
            context = ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER)
            context.load_cert_chain(*tls)
            self.server.socket = context.wrap_socket(self.server.socket, server_side=True)
        self.port = self.server.server_port
        self.thread = threading.Thread(target=self.server.serve_forever, daemon=True)
        self.thread.start()

    def close ( self ):

        self.server.shutdown()
        self.server.server_close()
        self.thread.join()

    def __enter__ ( self ): return self
    def __exit__ ( self, *_args ): self.close()


def pool ( first, second, policy="round_robin", extra="" ):

    return (
        f'add_upstream("api", "127.0.0.1:{first}")\n'
        f'add_upstream("api", "127.0.0.1:{second}")\n'
        f'set_balancer("api", {{ policy="{policy}", {extra} }})\n'
        'set_default_upstream("api")\n'
    )


class RoutingTests ( unittest.TestCase ):

    @classmethod
    def setUpClass ( cls ):

        cls.a, cls.b = Server("a"), Server("b")

    @classmethod
    def tearDownClass ( cls ):

        cls.a.close()
        cls.b.close()

    def test_host_path_method_rewrite_headers_and_no_fallback ( self ):

        extra = (
            f'add_upstream("api", "127.0.0.1:{self.b.port}")\n'
            'set_default_upstream(false)\n'
            'set_headers("request", {["X-Policy"]="global"})\n'
            'add_route {name="api", host="APP.TEST", path="/api", methods={"POST"}, upstream="api", '
            'strip_prefix=true, preserve_host=true, request_headers={["x-policy"]="route"}, '
            'response_headers={["x-service"]="api"}}\n'
            'add_route {name="private", path="/admin", deny=true}\n'
        )
        with Runtime(self.a.port, model=False, extra=extra) as proxy:
            connection = http.client.HTTPConnection("127.0.0.1", proxy.port)
            connection.request("POST", "/api/orders?q=one", b"hello", {"Host": "app.test:9090"})
            response = connection.getresponse()
            data = json.loads(response.read())
            self.assertEqual(response.status, 200)
            self.assertEqual(response.getheader("x-service"), "api")
            self.assertEqual((data["backend"], data["path"], data["body"]), ("b", "/orders?q=one", "hello"))
            headers = {key.lower(): value for key, value in data["headers"].items()}
            self.assertEqual(headers["x-policy"], "route")
            self.assertEqual(headers["host"], "app.test:9090")
            connection.close()
            self.assertEqual(proxy.request("/api", headers={"Host": "app.test"})[0], 404)
            self.assertEqual(proxy.request("/apix", body=b"x", headers={"Host": "app.test"})[0], 404)
            self.assertEqual(proxy.request("/%61dmin")[0], 403)
            self.assertEqual(proxy.request("/a/%2e%2e/admin")[0], 400)


    def test_protocol_close_decisions_are_preserved ( self ):

        for extra, framing, body in [
            ("set_runtime {keepalive_seconds=0}", "", b""),
            ("", "Content-Length: 9\r\nTransfer-Encoding: chunked\r\n", b"0\r\n\r\n"),
        ]:
            with Runtime(self.a.port, model=False, extra=extra) as proxy:
                stream = socket.create_connection(("127.0.0.1", proxy.port), timeout=3)
                stream.sendall(("POST / HTTP/1.1\r\nHost: localhost\r\n" + framing + "\r\n").encode() + body)
                response = http.client.HTTPResponse(stream)
                response.begin()
                self.assertEqual(response.status, 200)
                self.assertEqual(response.getheader("connection"), "close")
                response.read()
                self.assertEqual(stream.recv(1), b"")
                stream.close()

    def test_weighted_round_robin ( self ):

        extra = (
            f'add_upstream("api", {{address="127.0.0.1:{self.a.port}", weight=2}})\n'
            f'add_upstream("api", "127.0.0.1:{self.b.port}")\n'
            'set_default_upstream("api")\n'
            'add_route {name="inherited", path="/"}\n'
        )
        with Runtime(self.a.port, model=False, extra=extra) as proxy:
            counts = Counter(json.loads(proxy.request()[2])["backend"] for _ in range(12))
            self.assertEqual(counts, {"a": 8, "b": 4})

    def test_least_connections_and_global_capacity ( self ):

        with Runtime(self.a.port, model=False, extra=pool(self.a.port, self.b.port, "least_conn")) as proxy:
            connection = http.client.HTTPConnection("127.0.0.1", proxy.port)
            connection.request("GET", "/slow")
            response = connection.getresponse()
            first = response.read(1)
            second = json.loads(proxy.request()[2])
            original = json.loads(first + response.read())
            self.assertNotEqual(original["backend"], second["backend"])
            connection.close()
        with Runtime(self.a.port, model=False, extra="set_runtime {max_in_flight=1}") as proxy:
            connection = http.client.HTTPConnection("127.0.0.1", proxy.port)
            connection.request("GET", "/slow")
            response = connection.getresponse()
            response.read(1)
            self.assertEqual(proxy.request()[0], 503)
            response.read()
            connection.close()
            self.assertEqual(proxy.request()[0], 200)

    def test_passive_failure_and_safe_connect_retry ( self ):

        with Runtime(self.a.port, model=False, extra=pool(self.a.port, self.b.port, "first", "max_fails=1, cooldown_ms=5000")) as proxy:
            self.assertEqual(proxy.request("/fail")[0], 503)
            self.assertEqual(json.loads(proxy.request()[2])["backend"], "b")
        with Runtime(self.a.port, model=False, extra=pool(free_port(), self.b.port, "first", "connect_attempts=2")) as proxy:
            status, request_id, body = proxy.request(body=b"one operation")
            self.assertEqual((status, json.loads(body)["body"]), (200, "one operation"))
            events = until(lambda: proxy.terminal(request_id))
            self.assertEqual(sum(event["stage"] == "upstream_selected" for event in events), 2)
        with Runtime(self.a.port, model=False, extra=pool(self.a.port, self.b.port, "first", "connect_attempts=3")) as proxy:
            before = self.b.server.calls
            self.assertEqual(proxy.request("/drop", body=b"do not replay")[0], 502)
            self.assertEqual(self.b.server.calls, before)

    def test_active_tcp_health_recovers ( self ):

        port = free_port()
        extra = 'set_balancer("default", {health_interval_ms=100, health_timeout_ms=50})\n'
        # set_balancer creates a pool, so explicitly add its backend.
        extra += f'add_upstream("default", "127.0.0.1:{port}")\n'
        with Runtime(port, model=False, extra=extra) as proxy:
            until(lambda: proxy.request()[0] == 503)
            with Server("recovered", port=port):
                until(lambda: proxy.request()[0] == 200)
                self.assertEqual(json.loads(proxy.request()[2])["backend"], "recovered")

    def test_per_route_limits_and_capture ( self ):

        extra = (
            'add_route {name="limited", path="/limited", rate_limit_10s=1, max_body_bytes=4}\n'
            'add_route {name="silent", path="/silent", capture=false, model="off"}\n'
        )
        with Runtime(self.a.port, model=False, limits="rate_limit_10s=4", extra=extra) as proxy:
            self.assertEqual(proxy.request("/limited", body=b"12345")[0], 413)
            self.assertEqual(proxy.request("/limited")[0], 200)
            self.assertEqual(proxy.request("/limited")[0], 429)
            self.assertEqual(proxy.request("/")[0], 200)
            status, request_id, _ = proxy.request("/silent")
            self.assertEqual(status, 200)
            self.assertEqual(proxy.request("/")[0], 429)
            self.assertEqual(proxy.events(request_id), [])

    def test_reload_is_atomic_and_keeps_inflight_snapshot ( self ):

        with Runtime(self.a.port, model=False) as proxy:
            connection = http.client.HTTPConnection("127.0.0.1", proxy.port)
            connection.request("GET", "/slow")
            response = connection.getresponse()
            request_id = response.getheader("x-request-id")
            first = response.read(1)
            initial = proxy.config.read_text()
            proxy.config.write_text(initial.replace(str(self.a.port), str(self.b.port)))
            proxy.process.send_signal(signal.SIGHUP)
            until(lambda: json.loads(proxy.request()[2])["backend"] == "b")
            self.assertEqual(json.loads(first + response.read())["backend"], "a")
            connection.close()
            events = until(lambda: proxy.terminal(request_id))
            self.assertEqual(len({event["details"]["config_version"] for event in events}), 1)
            proxy.config.write_text("set_limits {typo=1}")
            proxy.process.send_signal(signal.SIGHUP)
            time.sleep(0.1)
            self.assertEqual(json.loads(proxy.request()[2])["backend"], "b")
            proxy.config.write_text(initial + "\nset_runtime {threads=3}")
            proxy.process.send_signal(signal.SIGHUP)
            time.sleep(0.1)
            self.assertEqual(json.loads(proxy.request()[2])["backend"], "b")

    def test_model_modes_are_actual_decisions ( self ):

        extra = (
            'set_model {mode="observe", threshold=0, allow_unvalidated=true}\n'
            'add_route {name="deny", path="/deny", deny=true}\n'
            'add_route {name="later", path="/later", model="background"}\n'
            'add_route {name="off", path="/off", model="off"}\n'
        )
        with Runtime(self.a.port, extra=extra) as proxy:
            self.assertEqual(proxy.request()[0], 200)
            status, request_id, _ = proxy.request("/deny")
            self.assertEqual(status, 403)
            self.assertFalse(any(event["stage"] == "upstream_selected" for event in until(lambda: proxy.terminal(request_id))))
            _, observed_id, _ = proxy.request()
            observed = until(lambda: proxy.terminal(observed_id))
            features = next(e["details"]["features"] for e in observed if e["stage"] == "inspected")
            self.assertEqual(features[8:10], [0, 0])
            status, request_id, _ = proxy.request("/later")
            self.assertEqual(status, 200)
            events = until(lambda: (items if items and items[-1]["stage"] == "analyzed" else None) if (items := proxy.events(request_id)) else None)
            self.assertLess(next(e["sequence"] for e in events if e["stage"] == "completed"), events[-1]["sequence"])
            self.assertIsInstance(events[-1]["details"]["risk_score"], float)
            _, request_id, _ = proxy.request("/off")
            events = until(lambda: proxy.terminal(request_id))
            self.assertEqual(next(e["details"]["model_state"] for e in events if e["stage"] == "inspected"), "off")

    def test_external_model_hash_is_checked ( self ):

        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            source = Path(__file__).resolve().parents[2] / "model/weights"
            for name in ("model.onnx", "metadata.json"): shutil.copy(source / name, root / name)
            config = root / "test.lua"
            config.write_text(f'set_model {{mode="observe", directory="{root}"}}')
            command = [str(BINARY), "--config", str(config), "--check"]
            self.assertEqual(subprocess.run(command, capture_output=True).returncode, 0)
            with (root / "model.onnx").open("ab") as output: output.write(b"tampered")
            self.assertNotEqual(subprocess.run(command, capture_output=True).returncode, 0)

    def test_tls_listener_and_verified_upstream ( self ):

        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            cert, key = root / "cert.pem", root / "key.pem"
            subprocess.run(["openssl", "req", "-x509", "-newkey", "rsa:2048", "-nodes", "-keyout", str(key),
                            "-out", str(cert), "-days", "1", "-subj", "/CN=localhost",
                            "-addext", "subjectAltName=DNS:localhost,IP:127.0.0.1"], check=True, capture_output=True)
            with Server("secure", tls=(cert, key)) as backend:
                upstream = (f'add_upstream("secure", {{address="127.0.0.1:{backend.port}", tls=true, '
                            f'server_name="localhost", ca_file="{cert}"}})\nset_default_upstream("secure")\n')
                extra = upstream + f'set_tls {{cert="{cert}", key="{key}"}}\n'
                with Runtime(self.a.port, model=False, extra=extra) as proxy:
                    connection = http.client.HTTPSConnection("127.0.0.1", proxy.port, context=ssl.create_default_context(cafile=cert))
                    connection.request("GET", "/")
                    response = connection.getresponse()
                    data = json.loads(response.read())
                    self.assertEqual((response.status, data["backend"]), (200, "secure"))
                    self.assertEqual(data["headers"]["x-forwarded-proto"], "https")
                    connection.close()
                probes = upstream + 'set_balancer("secure", {health_interval_ms=100, health_timeout_ms=300, health_path="/ready"})\n'
                before = backend.server.calls
                with Runtime(self.a.port, model=False, extra=probes) as proxy:
                    until(lambda: backend.server.calls > before)
                    self.assertEqual(proxy.request()[0], 200)
                with Runtime(self.a.port, model=False, extra=upstream.replace('server_name="localhost"', 'server_name="wrong.test"')) as proxy:
                    self.assertEqual(proxy.request()[0], 502)


if __name__ == "__main__":
    unittest.main(verbosity=2)
