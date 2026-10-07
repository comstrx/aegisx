"""Durable decision corruption and independent component policies over real HTTP."""
import json
import sqlite3
import unittest

from integration_support import Fixture
from lifecycle import until


class IntegrityTests ( Fixture, unittest.TestCase ):
    def test_corrupt_cold_record_obeys_failure_policy_and_recovers ( self ):
        with self.proxy('set_cache { decisions=true, on_lookup_failure="deny" }', control=True) as proxy:
            self.assertEqual(self.fetch(proxy.port)[0], 200)
            state = until(lambda: self.api("/state")[1] if self.api("/state")[1]["telemetry"]["completed"] else None)
            details = state["telemetry"]["recent"][0]["details"]
            action = {"config_version": state["config_version"], "route": details["route"], "actor": details["actor"],
                      "reason": "integrity fixture", "ttl_ms": 5000}
            key = self.api("/blocks", action)[1]["key"]
            self.assertEqual(self.fetch(proxy.port)[0], 403)
            with sqlite3.connect(proxy.database) as db:
                payload = json.loads(db.execute("SELECT payload FROM verdicts WHERE key=?", (key,)).fetchone()[0])
                payload["key"] = "f"*64
                db.execute("UPDATE verdicts SET payload=? WHERE key=?", (json.dumps(payload), key))
            self.assertEqual(self.api("/cache/purge", {"kind": "decisions"})[0], 200)
            self.assertEqual(self.fetch(proxy.port)[0], 503)
            self.assertEqual(self.api("/decisions")[0], 503)
            self.assertEqual(self.api("/blocks/revoke", {"key": key})[0], 200)
            self.assertEqual(self.fetch(proxy.port)[0], 200)
            self.assertGreater(self.api("/state")[1]["decisions"]["read_failures"], 0)

    def test_absent_journey_does_not_trigger_zero_component_threshold ( self ):
        with self.proxy('set_model { mode="observe", content_threshold=1, journey_threshold=0 }',
                        model=True, store=True) as proxy:
            status, request_id, _ = proxy.request()
            self.assertEqual(status, 200)
            event = until(lambda: next((event for event in proxy.events(request_id) if event["stage"] == "analyzed"), None))
            self.assertEqual(event["details"]["action"], "observed")
            self.assertEqual(event["details"]["triggered_signals"], {"content": False, "journey": False})
            self.assertEqual(event["details"]["thresholds"]["journey"], 0)

    def test_route_scalar_threshold_overrides_global_component_thresholds ( self ):
        settings = '''set_model { mode="observe", content_threshold=1, journey_threshold=1 }
add_route { name="override", path="/scope", threshold=0 }
'''
        with self.proxy(settings, model=True, store=True) as proxy:
            status, request_id, _ = proxy.request("/scope")
            self.assertEqual(status, 200)
            event = until(lambda: next((event for event in proxy.events(request_id) if event["stage"] == "analyzed"), None))
            self.assertEqual(event["details"]["action"], "observed_risk")
            self.assertEqual(event["details"]["thresholds"]["content"], 0)
            self.assertTrue(event["details"]["triggered_signals"]["content"])
            self.assertFalse(event["details"]["triggered_signals"]["journey"])


if __name__ == "__main__": unittest.main(verbosity=2)
