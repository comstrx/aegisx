"""Executed, application-disjoint counterfactual fixtures; labels never enter inputs."""
import hashlib
import json
import sqlite3


class Worlds:
    APPLICATIONS = {
        "atlas": ("train", "notes", "records", "authorize", "repository"),
        "mercury": ("train", "payments", "wallets", "permission", "ledger"),
        "cedar": ("train", "jobs", "deliveries", "access", "worker"),
        "birch": ("validation", "support", "tickets", "policy", "storage"),
        "lumen": ("calibration", "billing", "accounts", "authorize", "database"),
        "orbit": ("test", "exports", "documents", "permission", "repository"),
        "harbor": ("test", "events", "messages", "policy", "worker"),
    }

    @staticmethod
    def case ( application, index, authorized, secure, rollback, payload, operation ):
        split, service, table, check_name, writer = Worlds.APPLICATIONS[application]
        db = sqlite3.connect(":memory:")
        db.execute(f"CREATE TABLE {table}(id INTEGER PRIMARY KEY, owner INTEGER, value TEXT)")
        db.execute(f"INSERT INTO {table} VALUES(1,1,?)", ("private-record",))
        db.execute("CREATE TABLE effects(value TEXT)")
        db.commit()
        events = []
        def event ( name, state, span, parent="request" ):
            events.append({"service": service, "operation": name, "state": state,
                           "span_id": span, "parent_id": parent, "elapsed_ms": 0, "duration_ms": 0})
        event("http.request", "started", "request", None)
        actor = 1 if authorized else 2
        # Authorization derives from actual application state, not requested labels.
        allowed = db.execute(f"SELECT owner FROM {table} WHERE id=1").fetchone()[0] == actor
        def check ():
            event(check_name, "completed" if allowed else "failed", "policy")
        response = ""
        if secure: check()
        if not secure or allowed:
            if operation == "read":
                response = db.execute(f"SELECT value FROM {table} WHERE id=1").fetchone()[0]
                event(writer + ".read", "completed", "read")
            else:
                db.execute("INSERT INTO effects VALUES(?)", (payload,))
                event(writer + ".write", "completed", "write")
        if not secure: check()
        if rollback and not allowed:
            db.rollback()
            event("transaction.rollback", "completed", "finish")
        else:
            db.commit()
            event("transaction.commit", "completed", "finish")
        effects = db.execute("SELECT COUNT(*) FROM effects").fetchone()[0]
        # A rollback cannot undo data already returned; the oracle checks actual observable outcomes.
        violation = not allowed and (effects > 0 or response == "private-record")
        event("http.request", "completed", "request", None)
        route = hashlib.sha256(f"{application}:{index}".encode()).hexdigest()[:10]
        request = f"POST /api/{route}/{operation}\n" + json.dumps({"actor": actor, "object": 1, "value": payload})
        body = json.dumps({"status": "ok" if allowed else "denied", "data": response})
        db.close()
        return {"request": request, "response": body, "response_available": True, "events": events,
                "outcome": [200 if allowed else 403, 0, len(request.encode()), len(body.encode()), 1, 0, 1, 1],
                "label": int(violation), "origin": "recorded_lab", "group": f"world:{application}:{index}",
                "application": application, "split": split, "counterfactual": f"{application}:{index}:{actor}:{operation}:{rollback}",
                "oracle": {"allowed": allowed, "committed_effects": effects, "private_data_returned": response == "private-record"},
                "fixture": {"secure": secure, "rollback": rollback, "operation": operation}}

    @staticmethod
    def collect ( families=12 ):
        payloads = ["normal note", "SELECT best SQL books", "<script>alert('example')</script>",
                    "../../../docs/example.txt", "DROP TABLE examples; -- tutorial", '{"query":"$where is documented"}']
        rows = []
        for application in Worlds.APPLICATIONS:
            for index in range(families):
                payload = payloads[index % len(payloads)]
                for authorized in (False, True):
                    for secure in (False, True):
                        for rollback in (False, True):
                            for operation in ("read", "write"):
                                rows.append(Worlds.case(application, index, authorized, secure, rollback, payload, operation))
        return rows
