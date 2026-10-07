"""Deterministic streaming handshakes: the next chunk cannot exist until the client sees the previous one."""
import http.client
import threading
import unittest
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

from lifecycle import Runtime


class StreamingTests(unittest.TestCase):
    def test_buffered_headers_and_partial_body_do_not_wait_for_future_bytes(self):
        for chunked in (False, True):
            with self.subTest(chunked=chunked):
                first, last = threading.Event(), threading.Event()
                class Origin(BaseHTTPRequestHandler):
                    protocol_version = "HTTP/1.1"
                    def log_message(self, *_): pass
                    def do_GET(self):
                        self.send_response(200)
                        self.send_header("Transfer-Encoding" if chunked else "Content-Length", "chunked" if chunked else "11")
                        self.end_headers()
                        self.wfile.flush()
                        if not first.wait(5): return
                        try:
                            self.wfile.write(b"5\r\nfirst\r\n" if chunked else b"first")
                            self.wfile.flush()
                            if not last.wait(5): return
                            self.wfile.write(b"6\r\nsecond\r\n0\r\n\r\n" if chunked else b"second")
                            self.wfile.flush()
                        except (BrokenPipeError, ConnectionResetError): pass
                origin = ThreadingHTTPServer(("127.0.0.1", 0), Origin)
                thread = threading.Thread(target=origin.serve_forever, daemon=True)
                thread.start()
                try:
                    with Runtime(origin.server_port, model=False, store=False,
                                 extra="set_cache {decisions=false}\nset_runtime {write_buffer_bytes=4096}") as proxy:
                        client = http.client.HTTPConnection("127.0.0.1", proxy.port, timeout=2)
                        try:
                            client.request("GET", "/stream")
                            response = client.getresponse()
                            self.assertEqual(response.status, 200)
                            first.set()
                            self.assertEqual(response.read(5), b"first")
                            last.set()
                            self.assertEqual(response.read(), b"second")
                        finally:
                            first.set()
                            last.set()
                            client.close()
                finally:
                    first.set()
                    last.set()
                    origin.shutdown()
                    origin.server_close()
                    thread.join()


if __name__ == "__main__": unittest.main(verbosity=2)
