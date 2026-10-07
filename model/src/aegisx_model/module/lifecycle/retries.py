"""Executed retry/idempotency counterfactuals. The label comes from committed rows."""
import hashlib
import json
import sqlite3

from .worlds import Worlds


class Retries:
    @staticmethod
    def case ( application, index, retry, enforced, rollback ):
        split, service, _, check, writer = Worlds.APPLICATIONS[application]
        events = []
        def event ( operation, state, span, parent="request" ):
            events.append({"service": service, "operation": operation, "state": state,
                           "span_id": span, "parent_id": parent, "elapsed_ms": 0, "duration_ms": 0})
        with sqlite3.connect(":memory:") as db:
            db.execute("CREATE TABLE receipts(id TEXT PRIMARY KEY)")
            db.execute("CREATE TABLE effects(id TEXT)")
            event("http.request", "started", "request", None)
            for attempt in range(2 if retry else 1):
                span = f"attempt-{attempt}"
                event("job.execute", "started", span)
                duplicate = db.execute("SELECT 1 FROM receipts WHERE id=?", ("key",)).fetchone() is not None
                event(check + ".idempotency", "failed" if duplicate else "completed", f"check-{attempt}", span)
                if not (enforced and duplicate):
                    db.execute("INSERT INTO effects VALUES(?)", ("key",))
                    db.execute("INSERT OR IGNORE INTO receipts VALUES(?)", ("key",))
                    event(writer + ".write", "completed", f"write-{attempt}", span)
                if rollback:
                    db.rollback()
                    event("transaction.rollback", "completed", f"tx-{attempt}", span)
                else:
                    db.commit()
                    event("transaction.commit", "completed", f"tx-{attempt}", span)
                event("job.execute", "completed", span)
            effects = db.execute("SELECT COUNT(*) FROM effects").fetchone()[0]
        event("http.request", "completed", "request", None)
        route = hashlib.sha256(f"{application}:retry:{index}".encode()).hexdigest()[:10]
        request = f'POST /api/{route}/submit\n' + json.dumps({"idempotency_key": f"example-{index}", "value": "normal operation"})
        response = '{"accepted":true}'
        return {"request": request, "response": response, "response_available": True, "events": events,
                "outcome": [202, 0, len(request.encode()), len(response), 1, 0, 1, 1],
                "label": int(effects > 1), "origin": "recorded_lab", "application": application, "split": split,
                "group": f"transaction:{application}:{index}",
                "counterfactual": f"transaction:{application}:{index}:{retry}:{rollback}",
                "oracle": {"committed_effects": effects, "allowed_effects": 1},
                "fixture": {"retry": retry, "enforced": enforced, "rollback": rollback}}

    @staticmethod
    def collect ( families=12 ):
        return [Retries.case(application, index, retry, enforced, rollback)
                for application in Worlds.APPLICATIONS for index in range(families)
                for retry in (False, True) for enforced in (False, True) for rollback in (False, True)]
