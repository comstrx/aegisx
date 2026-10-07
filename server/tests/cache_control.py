import signal
import time
import unittest

from integration_support import Fixture, Origin
from lifecycle import until


class CacheTests ( Fixture, unittest.TestCase ):

    def test_public_cache_identity_expiry_and_policy ( self ):

        with self.proxy('set_cache { responses=true, response_ttl_ms=300, max_object_bytes=1024 }', control=True) as proxy:
            first = self.fetch(proxy.port, "/public")
            second = self.fetch(proxy.port, "/public")
            self.assertEqual(first[2], second[2])
            self.assertNotEqual(self.header(first[1], "x-request-id"), self.header(second[1], "x-request-id"))
            self.assertEqual(Origin.counts["/public"], 1)
            self.assertEqual(self.header(second[1], "age"), "0")
            time.sleep(0.35)
            self.fetch(proxy.port, "/public")
            self.assertEqual(Origin.counts["/public"], 2)
            for path in ("/private", "/no-store", "/cookie", "/vary", "/large"):
                self.fetch(proxy.port, path)
                self.fetch(proxy.port, path)
                self.assertEqual(Origin.counts[path], 2, path)
            for index, header in enumerate(({"Authorization":"secret"}, {"Cookie":"session=one"},
                                           {"Range":"bytes=0-2"}, {"Cache-Control":"no-cache"}, {"If-None-Match":"test"})):
                path = f"/bypass-{index}"
                self.fetch(proxy.port, path, headers=header)
                self.fetch(proxy.port, path, headers=header)
                self.assertEqual(Origin.counts[path], 2, header)
            self.assertGreaterEqual(self.api("/state")[1]["cache"]["response_hits"], 1)

    def test_cache_key_scope_reload_and_purge ( self ):

        extra = 'set_cache { responses=true }\nadd_route { name="uncached", path="/uncached", response_cache=false }'
        with self.proxy(extra, control=True) as proxy:
            for path, host in (("/key?a=1","one"), ("/key?a=2","one"), ("/key?a=1","two")):
                self.fetch(proxy.port, path, headers={"Host":host})
            self.assertEqual(Origin.counts["/key?a=1"], 2)
            for _ in range(2): self.fetch(proxy.port, "/uncached")
            self.assertEqual(Origin.counts["/uncached"], 2)
            self.fetch(proxy.port, "/purge")
            self.fetch(proxy.port, "/purge")
            self.assertEqual(self.api("/cache/purge", {"kind":"responses"})[0], 200)
            self.fetch(proxy.port, "/purge")
            self.assertEqual(Origin.counts["/purge"], 2)
            version = self.api("/state")[1]["config_version"]
            proxy.config.write_text(proxy.config.read_text() + '\nset_headers("response", {["x-generation"]="two"})\n')
            proxy.process.send_signal(signal.SIGHUP)
            until(lambda: self.api("/state")[1]["config_version"] != version)
            self.fetch(proxy.port, "/purge")
            self.assertEqual(Origin.counts["/purge"], 3)

    def test_successful_mutation_invalidates_cached_responses ( self ):

        with self.proxy('set_cache { responses=true }') as proxy:
            self.fetch(proxy.port, "/resource")
            self.fetch(proxy.port, "/resource")
            self.assertEqual(Origin.counts["/resource"], 1)
            self.assertEqual(self.fetch(proxy.port, "/resource", "POST", "update")[0], 204)
            self.fetch(proxy.port, "/resource")
            self.assertEqual(Origin.counts["/resource"], 2)

    def test_backend_signal_scoped_denial_and_ttl ( self ):

        extra = '''set_cache { decisions=true, deny_ttl_ms=250 }
set_identity { backend_block_header="x-backend-ban" }
add_route { name="other", path="/other" }
'''
        with self.proxy(extra, control=True) as proxy:
            first = self.fetch(proxy.port, "/signal")
            self.assertEqual(first[0], 200)
            self.assertIsNone(self.header(first[1], "x-backend-ban"))
            until(lambda: self.api("/decisions")[1]["items"])
            self.assertEqual(self.fetch(proxy.port, "/next")[0], 403)
            self.assertEqual(self.fetch(proxy.port, "/other")[0], 200)
            self.assertEqual(self.api("/state")[1]["decisions"]["hits"], 1)
            time.sleep(0.3)
            self.assertEqual(self.fetch(proxy.port, "/next")[0], 200)

    def test_control_auth_static_contracts_operator_block ( self ):

        with self.proxy('set_cache { decisions=true }', control=True) as proxy:
            self.assertEqual(self.api("/state", token="wrong")[0], 401)
            self.assertEqual(self.api("/state", headers={"Origin":"https://evil.example"})[0], 403)
            self.assertEqual(self.api("/state", headers={"Host":"evil.example"})[0], 400)
            status, _, body = self.fetch(self.admin)
            self.assertEqual(status, 200)
            self.assertIn(b"AegisX", body)
            self.assertEqual(self.fetch(self.admin, "/aegisx-bootstrap.json")[0], 200)
            self.assertEqual(self.api("/contracts")[1]["title"], "AegisX integration v1")
            self.assertEqual(self.fetch(self.admin, "/manage/v1/blocks", "POST", "x" * 5000)[0], 413)
            request_id = self.fetch(proxy.port)[1]
            state = until(lambda: self.api("/state")[1] if self.api("/state")[1]["telemetry"]["completed"] else None)
            event = state["telemetry"]["recent"][0]
            action = {"config_version":state["config_version"], "route":event["details"]["route"],
                      "actor":event["details"]["actor"], "reason":"test operator", "ttl_ms":1000,
                      "request_id": self.header(request_id,"x-request-id")}
            self.assertEqual(self.api("/blocks", {**action,"config_version":"stale"})[0], 409)
            self.assertEqual(self.api("/blocks", {**action,"actor":"!" * 64})[0], 400)
            self.assertEqual(self.api("/blocks", action)[0], 200)
            self.assertEqual(self.fetch(proxy.port)[0], 403)
            self.assertEqual(self.api("/cache/purge", {"kind":"decisions"})[0], 200)
            self.assertEqual(self.fetch(proxy.port)[0], 403)
            decisions = self.api("/decisions")[1]["items"]
            self.assertEqual(self.api("/blocks/revoke", {"key": decisions[0]["key"]})[0], 200)
            self.assertEqual(self.fetch(proxy.port)[0], 200)


if __name__ == "__main__": unittest.main(verbosity=2)
