"""Only the integrated backend can confirm safe cancellation."""
import os
import signal
import sqlite3
import subprocess
import unittest
from unittest.mock import patch
from integration_support import Fixture, TOKEN
from lifecycle import BINARY, until
from background_safety import BACKEND_TOKEN

class CancellationTests(Fixture, unittest.TestCase):
    def test_opt_in_durable_idempotent_backend_acknowledgement(self):
        with patch.dict(os.environ, {"AEGISX_BACKEND_TOKEN":BACKEND_TOKEN}):
            with self.proxy('add_route {name="jobs",path="/jobs",cancellation=true}',control=True,store=True,backend=True) as proxy:
                status,headers,_=self.fetch(proxy.port,"/jobs")
                self.assertEqual(status,200)
                request_id=self.header(headers,"x-request-id")
                command={"request_id":request_id,"route":"default"}
                self.assertEqual(self.api("/cancellations",command)[0],409)
                command["route"]="jobs"
                status,value=self.api("/cancellations",command)
                self.assertEqual(status,202)
                action=value["action"]
                self.assertEqual(action["state"],"requested")
                self.assertEqual(self.api("/cancellations",command)[1]["action"]["action_id"],action["action_id"])
                self.assertEqual(self.api("/backend/cancellations",token=TOKEN)[0],401)
                self.assertEqual(self.api("/backend/cancellations",token=BACKEND_TOKEN)[1]["items"][0]["state"],"requested")
                ack={"action_id":action["action_id"],"state":"accepted"}
                self.assertEqual(self.api("/backend/cancellations/ack",ack,token=BACKEND_TOKEN)[1]["action"]["state"],"accepted")
                self.assertEqual(self.api("/cancellations")[1]["items"][0]["state"],"accepted")
                ack["state"]="cancelled"
                self.assertEqual(self.api("/backend/cancellations/ack",ack,token=BACKEND_TOKEN)[0],200)
                self.assertEqual(self.api("/backend/cancellations/ack",ack,token=BACKEND_TOKEN)[0],200)
                self.assertEqual(self.api("/backend/cancellations/ack",{**ack,"state":"accepted"},token=BACKEND_TOKEN)[0],409)
                proxy.process.send_signal(signal.SIGTERM)
                proxy.process.wait(timeout=12)
                env=os.environ.copy()
                env["AEGISX_ADMIN_TOKEN"]=TOKEN
                proxy.process=subprocess.Popen([str(BINARY),"--config",str(proxy.config)],stdout=proxy.log,stderr=proxy.log,env=env)
                until(proxy.ready)
                self.assertEqual(self.api("/cancellations")[1]["items"][0]["state"],"cancelled")

    def test_repository_failure_allow_still_applies_explicit_rate_rules(self):
        for policy in ("allow","deny"):
            with self.subTest(policy=policy),self.proxy(f'set_cache {{decisions=true,on_lookup_failure="{policy}"}}\nset_limits {{rate_limit_10s=1}}',control=True,store=True) as proxy:
                # Fault injection; direct SQL changes are not a supported application write path.
                with sqlite3.connect(proxy.database) as database:
                    database.execute("ALTER TABLE verdicts RENAME TO unavailable_verdicts")
                self.assertEqual(self.fetch(proxy.port)[0],200 if policy=="allow" else 503)
                self.assertEqual(self.fetch(proxy.port)[0],429 if policy=="allow" else 503)

if __name__=="__main__": unittest.main(verbosity=2)
