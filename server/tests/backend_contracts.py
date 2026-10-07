import base64
import hashlib
import hmac
import json
import time
import unittest
import uuid

from integration_support import Fixture, Origin, SECRET
from lifecycle import until


class ContractTests ( Fixture, unittest.TestCase ):

    def test_route_header_overrides_ignore_case ( self ):
        extra = '''set_headers("request", {["x-policy"]="global"})
set_headers("response", {["x-policy"]="global"})
add_route {name="case",path="/case",request_headers={["X-Policy"]="route"},response_headers={["X-Policy"]="route"}}
'''
        with self.proxy(extra) as proxy:
            status, headers, _ = self.fetch(proxy.port, "/case")
            self.assertEqual(status, 200)
            self.assertEqual(self.header(headers, "x-policy"), "route")
            self.assertEqual(self.header(Origin.requests[-1][1], "x-policy"), "route")

    def test_identity_custom_headers_and_trust ( self ):

        identity = '''set_identity { request_id_header="x-correlation-id", actor_header="x-account",
trusted_peers={"127.0.0.1/32"}, preserve_trusted_id=true, forwarding=false }'''
        with self.proxy(identity + '\nset_cache { responses=true }', control=True) as proxy:
            expected = str(uuid.uuid4())
            for _ in range(2):
                result = self.fetch(proxy.port, "/actor", headers={"X-Correlation-ID":expected, "X-Account":"private-account",
                    "X-Forwarded-For":"forged", "X-Request-ID":"forged"})
                self.assertEqual(self.header(result[1], "x-correlation-id"), expected)
                self.assertIsNone(self.header(result[1], "x-request-id"))
            self.assertEqual(Origin.counts["/actor"], 2)
            forwarded = Origin.requests[-1][1]
            self.assertIsNone(self.header(forwarded, "x-forwarded-for"))
            self.assertEqual(self.header(forwarded,"x-account"), "private-account")
            self.assertNotIn("private-account", json.dumps(self.api("/state")[1]))
        untrusted = identity.replace('127.0.0.1/32','192.0.2.0/24')
        with self.proxy(untrusted) as proxy:
            result = self.fetch(proxy.port, headers={"X-Correlation-ID":expected, "X-Account":"forged"})
            self.assertNotEqual(self.header(result[1],"x-correlation-id"), expected)
            self.assertIsNone(self.header(Origin.requests[-1][1], "x-account"))

    def test_signed_webhook_retries_stable_id_and_no_storm ( self ):

        extra = f'''set_cache {{ decisions=true, deny_ttl_ms=5000 }}
set_identity {{ backend_block_header="x-backend-ban" }}
set_webhooks {{ enabled=true, attempts=3, endpoints={{
 {{ name="jobs", url="http://127.0.0.1:{self.upstream}/retry", secret_env="AEGISX_TEST_HOOK_KEY", events={{"backend_signal"}} }}
}} }}'''
        with self.proxy(extra, control=True) as proxy:
            first = self.fetch(proxy.port, "/signal")
            request_id = self.header(first[1], "x-request-id")
            until(lambda: len(Origin.hooks) == 2)
            first_id = None
            for headers, body in Origin.hooks:
                event_id = self.header(headers,"webhook-id")
                timestamp = self.header(headers,"webhook-timestamp")
                expected = "v1," + base64.b64encode(hmac.new(SECRET.encode(), f"{event_id}.{timestamp}.".encode()+body, hashlib.sha256).digest()).decode()
                self.assertTrue(hmac.compare_digest(self.header(headers,"webhook-signature"), expected))
                self.assertEqual(json.loads(body)["request_id"], request_id)
                if first_id: self.assertEqual(event_id, first_id)
                first_id = event_id
            for _ in range(5): self.assertEqual(self.fetch(proxy.port)[0], 403)
            time.sleep(0.1)
            self.assertEqual(len(Origin.hooks), 2)
            until(lambda: self.api("/state")[1]["webhooks"]["delivered"] == 1)

    def test_permanent_webhook_failure_is_not_retried ( self ):

        extra = f'''add_route {{ name="deny", path="/", deny=true }}
set_webhooks {{ enabled=true, attempts=3, endpoints={{
 {{ name="reject", url="http://127.0.0.1:{self.upstream}/permanent", secret_env="AEGISX_TEST_HOOK_KEY" }}
}} }}'''
        with self.proxy(extra, control=True) as proxy:
            self.assertEqual(self.fetch(proxy.port)[0], 403)
            until(lambda: self.api("/state")[1]["webhooks"]["failed"] == 1)
            self.assertEqual(len(Origin.hooks), 1)

    def test_background_analysis_with_durable_decisions_and_summary_capture ( self ):

        extra = '''set_model { mode="background", threshold=0, allow_unvalidated=true }
set_cache { decisions=true, background_denials=true }
set_telemetry { capture="summary" }'''
        with self.proxy(extra, control=True) as proxy:
            first = self.fetch(proxy.port)
            self.assertEqual(first[0], 200)
            until(lambda: any(event["stage"] == "analyzed" for event in self.api("/state")[1]["telemetry"]["recent"]))
            self.assertEqual(self.fetch(proxy.port)[0], 403)
            self.assertTrue(proxy.database.exists())
            stages = {event["stage"] for event in self.api("/state")[1]["telemetry"]["recent"]}
            self.assertNotIn("received", stages)
            self.assertIn("analyzed", stages)

    def test_optional_services_stay_disabled ( self ):

        with self.proxy('set_cache { decisions=false, responses=false }\nset_identity { propagate=false }\nset_telemetry { enabled=false }', control=True) as proxy:
            for _ in range(2):
                response = self.fetch(proxy.port)
                self.assertEqual(response[0], 200)
                self.assertIsNone(self.header(response[1], "x-request-id"))
            state = self.api("/state")[1]
            self.assertEqual(Origin.counts["/"], 2)
            self.assertEqual(state["telemetry"]["total"], 0)
            self.assertEqual(state["telemetry"]["recent"], [])
            self.assertEqual(state["webhooks"]["accepted"], 0)


if __name__ == "__main__": unittest.main(verbosity=2)
