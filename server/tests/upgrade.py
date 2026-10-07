"""Verify a real old database/decision survives an executable upgrade."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import signal
import sqlite3
import subprocess

import lifecycle
from integration_support import Fixture, TOKEN

parser = argparse.ArgumentParser()
parser.add_argument("--baseline", type=Path, required=True)
parser.add_argument("--candidate", type=Path, required=True)
parser.add_argument("--output", type=Path, required=True)
args = parser.parse_args()
baseline, candidate = args.baseline.resolve(), args.candidate.resolve()
fixture = Fixture()
fixture.setUpClass()
fixture.setUp()
lifecycle.BINARY = baseline
try:
    with fixture.proxy('set_cache {decisions=true,deny_ttl_ms=600000}', control=True, store=True) as proxy:
        status, headers, _ = fixture.fetch(proxy.port)
        assert status == 200
        request_id = fixture.header(headers, "x-request-id")
        lifecycle.until(lambda: proxy.terminal(request_id))
        state = fixture.api("/state")[1]
        baseline_version = state["version"]
        event = next(item for item in state["telemetry"]["recent"] if item["request_id"] == request_id and item["details"].get("actor"))
        status, result = fixture.api("/blocks", {"config_version": state["config_version"],
            "route": event["details"]["route"], "actor": event["details"]["actor"],
            "ttl_ms": 60000, "reason": "upgrade-regression"})
        assert status == 200
        key = result["key"]
        proxy.process.send_signal(signal.SIGTERM)
        assert proxy.process.wait(timeout=12) == 0
        with proxy.config.open("a") as config: config.write("\nset_queue {capacity=8,timeout_ms=1000}\n")
        proxy.process = subprocess.Popen([str(candidate), "--config", str(proxy.config)],
            stdout=proxy.log, stderr=proxy.log, env=dict(os.environ, AEGISX_ADMIN_TOKEN=TOKEN))
        lifecycle.until(proxy.ready)
        assert fixture.fetch(proxy.port)[0] == 403
        assert fixture.api("/decisions")[1]["items"][0]["key"] == key
        state = fixture.api("/state")[1]
        assert state["queue"]["capacity"] == 8
        with sqlite3.connect(proxy.database) as database:
            retained = database.execute("SELECT COUNT(*) FROM events WHERE request_id=?", (request_id,)).fetchone()[0]
            assert retained > 0
        report = {"baseline_version": baseline_version, "candidate_version": state["version"],
            "baseline_sha256": hashlib.sha256(baseline.read_bytes()).hexdigest(),
            "candidate_sha256": hashlib.sha256(candidate.read_bytes()).hexdigest(),
            "existing_ban_enforced": True, "legacy_request_events": retained,
            "new_queue_enabled": True}
        args.output.write_text(json.dumps(report, indent=2) + "\n")
        print(json.dumps(report))
finally:
    fixture.tearDownClass()
