"""Recorded loopback requests against an isolated instrumented SQLite application.

Labels come from the scenario oracle and actual committed state, never model output.
This is a controlled lab, not production attack telemetry.
"""
import hashlib
import http.client
import json
import random
import sqlite3
import threading
import time
from http.server import BaseHTTPRequestHandler, HTTPServer


class Lab:
    @staticmethod
    def collect ( count=4096, seed=73 ):
        records = []
        store = {}
        randomizer = random.Random(seed)

        class Handler ( BaseHTTPRequestHandler ):
            def log_message ( self, *args ): pass

            def do_POST ( self ):
                started = time.perf_counter()
                events = []
                def event ( operation, state, span, parent=None ):
                    events.append({"service": "application", "operation": operation, "state": state,
                                   "span_id": span, "parent_id": parent, "duration_ms": 0,
                                   "elapsed_ms": int((time.perf_counter() - started) * 1000)})
                body = self.rfile.read(int(self.headers.get("Content-Length", 0)))
                data = json.loads(body)
                # Scenario controls stay in the fixture; never in model input.
                case = store["case"]
                database = sqlite3.connect(":memory:")
                database.execute("CREATE TABLE records(id INTEGER PRIMARY KEY, owner INTEGER, value TEXT)")
                database.executemany("INSERT INTO records VALUES(?,?,?)", [(1, 1, "public note"), (2, 2, "private record")])
                database.commit()
                event("http.request", "started", "root")
                event("authentication", "started", "auth", "root")
                event("authentication", "completed", "auth", "root")
                status, result = 200, {"ok": True}
                violation = False
                kind = case["kind"]
                if kind == "search":
                    event("repository.read", "started", "db", "root")
                    query = data["query"]
                    try:
                        # Deliberately insecure READ-ONLY lab variant; never external data or processes.
                        rows = database.execute("SELECT id,value FROM records WHERE owner=1 AND value LIKE '%" + query + "%'").fetchall() if case["unsafe"] else database.execute("SELECT id,value FROM records WHERE owner=1 AND value LIKE ?", ("%" + query + "%",)).fetchall()
                        result = {"items": rows}
                        violation = any(row[0] == 2 for row in rows)
                        event("repository.read", "completed", "db", "root")
                    except sqlite3.Error:
                        status, result = 400, {"error": "invalid query"}
                        event("repository.read", "failed", "db", "root")
                else:
                    owner = database.execute("SELECT owner FROM records WHERE id=?", (data["id"],)).fetchone()[0]
                    allowed = owner == data["actor"]
                    def authorization ():
                        event("authorization", "started", "policy", "root")
                        event("authorization", "completed" if allowed else "failed", "policy", "root")
                    def write ():
                        event("repository.write", "started", "db", "root")
                        database.execute("UPDATE records SET value=? WHERE id=?", (data["value"], data["id"]))
                        database.commit()
                        event("repository.write", "completed", "db", "root")
                    if case["late"]:
                        write()
                        authorization()
                    else:
                        authorization()
                        if allowed: write()
                    changed = database.execute("SELECT value FROM records WHERE id=?", (data["id"],)).fetchone()[0] == data["value"]
                    violation = changed and not allowed
                    # Same HTTP response can follow early safe rejection or too-late rejection.
                    status = 200 if allowed else 403
                    result = {"ok": allowed}
                event("http.request", "completed", "root")
                response = json.dumps(result, separators=(",", ":")).encode()
                elapsed = int((time.perf_counter() - started) * 1000)
                database.close()
                store["observed"] = {"events": events, "label": int(violation), "status": status, "elapsed": elapsed}
                self.send_response(status)
                self.send_header("Content-Type", "application/json")
                self.send_header("Content-Length", str(len(response)))
                self.end_headers()
                self.wfile.write(response)

        server = HTTPServer(("127.0.0.1", 0), Handler)
        thread = threading.Thread(target=server.serve_forever, daemon=True)
        thread.start()
        try:
            for index in range(count):
                family = index % 128
                search = family % 4 == 0
                unsafe = bool((index // 128) % 2)
                suffix = hashlib.sha256(f"{seed}:{family}".encode()).hexdigest()[:8]
                if search:
                    query = randomizer.choice(["note", "missing", "' OR 1=1 --", "' UNION SELECT id,value FROM records --", "O'Reilly"])
                    data = {"query": query, "page": index % 3}
                    case = {"kind": "search", "unsafe": unsafe}
                else:
                    data = {"id": 2, "actor": 1 if family % 3 else 2, "value": "update-" + hashlib.sha256(f"value:{seed}:{family}".encode()).hexdigest()[:12]}
                    case = {"kind": "write", "late": unsafe}
                path = "/api/" + suffix + ("/search" if search else "/records")
                body = json.dumps(data, separators=(",", ":")).encode()
                store["case"] = case
                connection = http.client.HTTPConnection(*server.server_address, timeout=3)
                connection.request("POST", path, body, {"Content-Type": "application/json"})
                response = connection.getresponse()
                raw = response.read()
                connection.close()
                observed = store["observed"]
                request = b"POST " + path.encode() + b"\n" + body
                records.append({"request": request.decode(), "response": raw.decode(), "events": observed["events"],
                                "outcome": [response.status, observed["elapsed"], len(body), len(raw), 1, 0, 1, 1],
                                "response_available": True, "label": observed["label"], "origin": "recorded_lab",
                                "group": f"lab-family-{family}", "scenario": case["kind"],
                                "label_basis": "actual foreign row read or unauthorized committed write"})
        finally:
            server.shutdown()
            server.server_close()
            thread.join()
        return records
