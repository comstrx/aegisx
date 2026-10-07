"""Executed SQLite transaction contrasts. No network targets or fabricated outcomes."""
import hashlib
import json
import sqlite3


class Transactions:
    @staticmethod
    def collect ( families=256 ):
        rows = []
        for family in range(families):
            route = hashlib.sha256(f"transaction-route:{family}".encode()).hexdigest()[:10]
            # Names vary by family, independently of outcome and safety configuration.
            service = ("billing", "wallet", "ledger", "payments")[family % 4]
            for actor in (1, 2):
                for amount in (-10, 10, 1000):
                    for early in (False, True):
                        for rollback in (False, True):
                            db = sqlite3.connect(":memory:")
                            db.execute("CREATE TABLE accounts(id INTEGER PRIMARY KEY, owner INTEGER, balance INTEGER)")
                            db.executemany("INSERT INTO accounts VALUES(?,?,?)", [(1, 1, 100), (2, 2, 100)])
                            db.commit()
                            events = []
                            def event ( operation, state, span, parent="root", service=service, events=events ):
                                events.append({"service": service, "operation": operation, "state": state,
                                               "span_id": span, "parent_id": parent, "duration_ms": 0, "elapsed_ms": 0})
                            event("http.request", "started", "root", None)
                            event("authentication", "completed", "identity")
                            allowed = actor == 1 and 0 < amount <= 100
                            def policy ( event=event, allowed=allowed ):
                                event("policy.validate", "started", "check")
                                event("policy.validate", "completed" if allowed else "failed", "check")
                            def write ( event=event, db=db, amount=amount ):
                                event("transaction.begin", "completed", "transaction")
                                event("repository.write", "started", "update", "transaction")
                                db.execute("UPDATE accounts SET balance=balance-? WHERE id=1", (amount,))
                                db.execute("UPDATE accounts SET balance=balance+? WHERE id=2", (amount,))
                                event("repository.write", "completed", "update", "transaction")
                            if early:
                                policy()
                                if allowed: write()
                            else:
                                write()
                                policy()
                            if not allowed and rollback:
                                db.rollback()
                                event("transaction.rollback", "completed", "end")
                            else:
                                db.commit()
                                event("transaction.commit", "completed", "end")
                            balances = [value[0] for value in db.execute("SELECT balance FROM accounts ORDER BY id")]
                            violated = not allowed and balances != [100, 100]
                            status = 200 if allowed else 403
                            body = json.dumps({"actor": actor, "from": 1, "to": 2, "amount": amount}, separators=(",", ":"))
                            response = json.dumps({"ok": allowed, "balances": balances}, separators=(",", ":"))
                            event("http.request", "completed", "root", None)
                            rows.append({"request": f"POST /api/{route}/transfer\n{body}", "response": response,
                                         "events": events, "response_available": True,
                                         "outcome": [status, 0, len(body), len(response), 1, 0, 1, 1],
                                         "label": int(violated), "origin": "recorded_lab", "scenario": "transaction",
                                         "group": f"transaction-family-{family}",
                                         "label_basis": "unauthorized, negative or excessive transfer actually committed",
                                         "fixture": {"early": early, "rollback": rollback}})
                            db.close()
        return rows
