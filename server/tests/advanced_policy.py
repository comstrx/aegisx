import unittest
from concurrent.futures import ThreadPoolExecutor
from collections import Counter

from integration_support import Fixture, Origin
from lifecycle import until


class PolicyTests ( Fixture, unittest.TestCase ):

    def test_wildcard_header_routes_and_exact_host_priority ( self ):

        extra = '''add_route { name="wild", host="*.example.test", path="/blocked", deny=true }
add_route { name="exact", host="safe.example.test", path="/" }
add_route { name="preview", path="/release", match_headers={ ["x-release"]="preview" },
    response_headers={ ["x-selected"]="preview" } }
add_route { name="stable", path="/release", response_headers={ ["x-selected"]="stable" } }
'''
        with self.proxy(extra) as proxy:
            self.assertEqual(self.fetch(proxy.port, "/blocked", headers={"Host":"a.example.test"})[0], 403)
            self.assertEqual(self.fetch(proxy.port, "/blocked", headers={"Host":"safe.example.test"})[0], 200)
            self.assertEqual(self.fetch(proxy.port, "/blocked", headers={"Host":"example.test"})[0], 200)
            self.assertEqual(self.fetch(proxy.port, "/blocked", headers={"Host":"evilexample.test"})[0], 200)
            self.assertEqual(self.header(self.fetch(proxy.port, "/release", headers={"X-Release":"preview"})[1],"x-selected"), "preview")
            self.assertEqual(self.header(self.fetch(proxy.port, "/release")[1],"x-selected"), "stable")

    def test_http_health_rejects_live_but_unhealthy_application ( self ):

        extra = 'set_balancer("default", { health_interval_ms=100, health_timeout_ms=100, health_path="/health", health_status=200 })'
        with self.proxy(extra, control=True) as proxy:
            try:
                until(lambda: Origin.counts["/health"] > 0)
                Origin.health_status = 503
                until(lambda: not self.api("/state")[1]["upstreams"][0]["backends"][0]["healthy"])
                self.assertEqual(self.fetch(proxy.port)[0], 503)
                Origin.health_status = 200
                until(lambda: self.api("/state")[1]["upstreams"][0]["backends"][0]["healthy"])
                self.assertEqual(self.fetch(proxy.port)[0], 200)
            finally:
                Origin.health_status = 200

    def test_route_limits_run_before_response_cache_hits ( self ):

        extra = 'set_cache { responses=true }\nadd_route { name="limited", path="/limited", rate_limit_10s=1 }'
        with self.proxy(extra) as proxy:
            self.assertEqual(self.fetch(proxy.port, "/limited")[0], 200)
            self.assertEqual(self.fetch(proxy.port, "/limited")[0], 429)
            self.assertEqual(Origin.counts["/limited"], 1)

    def test_explicit_model_decision_cache ( self ):

        extra = '''set_model { mode="background", threshold=0, allow_unvalidated=true }
set_cache { decisions=true, background_denials=true }'''
        with self.proxy(extra, control=True) as proxy:
            self.assertEqual(self.fetch(proxy.port)[0], 200)
            until(lambda: self.api("/decisions")[1]["items"])
            self.assertEqual(self.fetch(proxy.port)[0], 403)
            state = self.api("/state")[1]
            self.assertEqual(state["decisions"]["hits"], 1)
            self.assertEqual(Origin.counts["/"], 1)


    def test_indexed_routes_and_effective_worker_configuration ( self ):
        extra = """set_runtime { threads=2, work_stealing=false, accept_tasks=2, upstream_keepalive_capacity=256 }
for index=1,512 do
    add_route { name="route-" .. index, path="/service/" .. index, exact=true, response_headers={["x-route"]="" .. index} }
end
"""
        with self.proxy(extra, control=True) as proxy:
            self.assertEqual(self.header(self.fetch(proxy.port, "/service/512")[1], "x-route"), "512")
            self.assertIsNone(self.header(self.fetch(proxy.port, "/service/512/child")[1], "x-route"))
            config = self.api("/state")[1]["configuration"]
            self.assertEqual(config["route_count"], 512)
            self.assertEqual(config["accept_tasks"], 2)
            self.assertEqual(config["upstream_keepalive_capacity"], 256)
            self.assertFalse(config["work_stealing"])
            self.assertEqual(config["feature_count"], 296)
            self.assertNotIn("token_env", config)


    def test_worker_topologies_share_rate_policy ( self ):
        for mode in ("false", "true"):
            with self.subTest(work_stealing=mode):
                extra = f"set_runtime {{threads=4,work_stealing={mode}}}\nset_limits {{rate_limit_10s=5}}"
                with self.proxy(extra) as proxy:
                    with ThreadPoolExecutor(max_workers=8) as clients:
                        statuses = list(clients.map(lambda _: self.fetch(proxy.port, "/shared")[0], range(20)))
                    self.assertEqual(Counter(statuses), {200:5,429:15})


if __name__ == "__main__": unittest.main(verbosity=2)
