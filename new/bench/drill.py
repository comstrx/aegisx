#!/usr/bin/env python3

import argparse
import http.client
import json
import os
import re
import secrets
import signal
import subprocess
import tempfile
import time

from pathlib import Path

SSH = ["ssh", "-o", "BatchMode=yes", "-o", "StrictHostKeyChecking=no"]
PORTS = [3800, 3801]

CONFIG = """
set_listen("{bind}:{listen}")
set_runtime {{ workers = {workers} }}
add_upstream("default", "{backend}:{first}")
add_upstream("default", "{backend}:{second}")
set_balancer("default", {{ policy = "{policy}", attempts = 2, max_fails = 3, cooldown_ms = 2000, retry_on = {{ {retry} }}, health = {{ path = "/ready", interval_ms = 500, timeout_ms = 400 }} }})
set_limits {{ timeout_ms = 5000 }}
set_control {{ enabled = true, listen = "127.0.0.1:{control}", token_env = "AEGISX_DRILL_TOKEN" }}
set_telemetry {{ recent = 400, journeys = 400 }}
set_log {{ level = "warn" }}
add_route {{ name = "orders", path = "/api/orders", capture = true }}
add_route {{ name = "slow", path = "/api/slow", capture = true, timeout_ms = 500 }}
add_route {{ name = "flaky", path = "/api/flaky", capture = true }}
add_route {{ name = "all", path = "/" }}
"""


class Lab:

    def __init__ ( self, options ):

        self.options = options
        self.root = Path(tempfile.mkdtemp(prefix = "drill-"))
        self.config = self.root / "drill.lua"
        self.token = secrets.token_hex(32)
        self.listen, self.control = 18080, 18090
        self.policy, self.retry = "round_robin", '"connect", "error"'
        self.process = None
        self.report = {"scenarios": {}}

    def shell ( self, host, command, check = True ):

        return subprocess.run([*SSH, f"ubuntu@{host}", command], capture_output = True, text = True, check = check).stdout

    def app ( self, port, up ):

        if up:

            self.shell(self.options.backend, f"APP_LISTEN=0.0.0.0:{port} APP_WORKERS=2 APP_POOL=12 nohup ./bin/app > results/app-{port}.log 2>&1 < /dev/null & echo $! > results/app-{port}.pid; for _ in $(seq 1 40); do ss -ltn | grep -q ':{port} ' && break; sleep 0.25; done")

        else:

            self.shell(self.options.backend, f"kill -9 $(cat results/app-{port}.pid) 2>/dev/null; true", check = False)

    def write ( self ):

        self.config.write_text(CONFIG.format(bind = self.options.bind, listen = self.listen, workers = self.options.workers, backend = self.options.backend, first = PORTS[0], second = PORTS[1], policy = self.policy, retry = self.retry, control = self.control))

    def start ( self ):

        self.write()
        self.log = open(self.root / "aegisx.log", "a")
        self.process = subprocess.Popen([self.options.aegisx, "--config", str(self.config)], stdout = self.log, stderr = self.log, env = {**os.environ, "AEGISX_DRILL_TOKEN": self.token})

        for _ in range(100):

            try:

                if self.admin("GET", "/state")[0] == 200 and self.get("/health")["status"] == 200: return

            except OSError: pass

            time.sleep(0.1)

        raise RuntimeError("aegisx did not start")

    def admin ( self, method, path, body = None ):

        client = http.client.HTTPConnection("127.0.0.1", self.control, timeout = 5)
        client.request(method, f"/api/v1{path}", body = body, headers = {"Authorization": f"Bearer {self.token}", "Host": f"127.0.0.1:{self.control}", **({"Content-Type": "application/json"} if body else {})})
        reply = client.getresponse()
        data = reply.read()
        client.close()

        return reply.status, data

    def get ( self, path, method = "GET", headers = None, body = None, timeout = 10 ):

        client = http.client.HTTPConnection(self.options.bind, self.listen, timeout = timeout)
        started = time.monotonic()
        client.request(method, path, body = body, headers = headers or {})
        reply = client.getresponse()
        first = time.monotonic() - started
        data = reply.read()
        total = time.monotonic() - started
        client.close()

        return {"status": reply.status, "headers": dict(reply.getheaders()), "body": data, "first_ms": first * 1000, "total_ms": total * 1000}

    def counters ( self ):

        text = self.admin("GET", "/metrics")[1].decode()
        found = {}

        for name, port, value in re.findall(r'aegisx_backend_(\w+?)(?:_total)?\{pool="default",backend="[\d.]+:(\d+)"\} (\d+)', text):

            found.setdefault(int(port), {})[name] = int(value)

        return found

    def delta ( self, before, after ):

        return {port: {name: after[port][name] - before[port].get(name, 0) for name in ("responses", "failures", "retries")} for port in after}

    def wrk ( self, seconds, connections = 64, script = "load/mix.lua" ):

        return subprocess.Popen([*SSH, f"ubuntu@{self.options.load}", f"wrk -t2 -c{connections} -d{seconds}s --latency -s {script} http://{self.options.bind}:{self.listen}/"], stdout = subprocess.PIPE, stderr = subprocess.STDOUT, text = True)

    def loaded ( self, process ):

        output = process.communicate()[0]

        def number ( pattern ):

            found = re.search(pattern, output)

            return float(found.group(1)) if found else 0.0

        errors = re.search(r"Socket errors: connect (\d+), read (\d+), write (\d+), timeout (\d+)", output)

        return {
            "requests": int(number(r"(\d+) requests in")),
            "rps": number(r"Requests/sec:\s+([\d.]+)"),
            "non_2xx": int(number(r"Non-2xx or 3xx responses: (\d+)")),
            "socket_errors": dict(zip(("connect", "read", "write", "timeout"), map(int, errors.groups()))) if errors else {},
            "p99": (re.search(r"^\s+99%\s+(\S+)", output, re.M) or [None, None])[1],
        }

    def until ( self, port, up, limit = 15.0 ):

        started = time.monotonic()

        while time.monotonic() - started < limit:

            if self.counters().get(port, {}).get("up") == int(up): return round((time.monotonic() - started) * 1000)

            time.sleep(0.05)

        return None

    def reload ( self, policy = None, retry = None ):

        self.policy, self.retry = policy or self.policy, retry or self.retry
        self.write()

        status = self.admin("POST", "/reload", b"{}")[0]

        for _ in range(60):

            state = json.loads(self.admin("GET", "/state")[1])

            if state["upstreams"][0]["policy"].lower() == self.policy.replace("_", ""): break

            time.sleep(0.05)

        return status

    def journey ( self, reply ):

        id = reply["headers"].get("x-request-id") or reply["headers"].get("X-Request-Id")

        if not id: return None

        time.sleep(0.05)

        events = json.loads(self.admin("GET", f"/requests/{id}")[1]).get("events", [])

        return {"id": id, "stages": [{"stage": event["stage"], "elapsed_ms": event["elapsed_ms"], **event["details"]} for event in events]}

    def stop ( self ):

        if self.process and self.process.poll() is None:

            self.process.send_signal(signal.SIGTERM)

            try: self.process.wait(timeout = 30)
            except subprocess.TimeoutExpired: self.process.kill()


def spread ( lab ):

    result = {}

    for policy in ("round_robin", "least_conn", "least_time", "random"):

        lab.reload(policy = policy)

        before = lab.counters()
        load = lab.loaded(lab.wrk(8))
        moved = lab.delta(before, lab.counters())
        total = max(sum(counts["responses"] for counts in moved.values()), 1)

        result[policy] = {**load, "share": {port: round(counts["responses"] * 100 / total, 1) for port, counts in moved.items()}, "failures": sum(counts["failures"] for counts in moved.values())}

    lab.reload(policy = "round_robin")

    return result


def skew ( lab ):

    result = {}

    for policy in ("round_robin", "least_conn", "least_time"):

        lab.reload(policy = policy)

        before = lab.counters()
        process = subprocess.Popen([*SSH, f"ubuntu@{lab.options.backend}", f"timeout 10 wrk -t1 -c24 -d9s 'http://127.0.0.1:{PORTS[1]}/api/catalog?page=1&size=100&q=e' > /dev/null 2>&1; true"])

        time.sleep(0.5)

        load = lab.loaded(lab.wrk(8))
        process.wait()
        moved = lab.delta(before, lab.counters())
        total = max(sum(counts["responses"] for counts in moved.values()), 1)

        result[policy] = {"rps": load["rps"], "p99": load["p99"], "non_2xx": load["non_2xx"], "share": {port: round(counts["responses"] * 100 / total, 1) for port, counts in moved.items()}}

    lab.reload(policy = "round_robin")

    return result


def failover ( lab ):

    before = lab.counters()
    process = lab.wrk(20)

    time.sleep(5)
    lab.app(PORTS[1], False)

    killed = time.monotonic()
    down = lab.until(PORTS[1], False)

    time.sleep(max(0.0, 7 - (time.monotonic() - killed)))
    lab.app(PORTS[1], True)

    up = lab.until(PORTS[1], True)
    load = lab.loaded(process)
    moved = lab.delta(before, lab.counters())

    return {**load, "marked_down_ms": down, "marked_up_ms": up, "backends": moved}


def blackout ( lab ):

    for port in PORTS: lab.app(port, False)

    time.sleep(0.3)

    dark = [lab.get("/api/catalog?page=1&size=5") for _ in range(20)]

    for port in PORTS: lab.app(port, True)

    started = time.monotonic()
    recovered = None

    while time.monotonic() - started < 15:

        if lab.get("/api/catalog?page=1&size=5")["status"] == 200: recovered = round((time.monotonic() - started) * 1000); break

        time.sleep(0.05)

    statuses = {}

    for reply in dark: statuses[reply["status"]] = statuses.get(reply["status"], 0) + 1

    return {"statuses_while_dark": statuses, "slowest_refusal_ms": round(max(reply["total_ms"] for reply in dark), 1), "recovered_after_start_ms": recovered}


def slow ( lab ):

    reply = lab.get("/api/slow?ms=1500")
    fine = lab.get("/api/slow?ms=100")

    return {"status": reply["status"], "total_ms": round(reply["total_ms"]), "journey": lab.journey(reply), "within_budget": {"status": fine["status"], "total_ms": round(fine["total_ms"])}}


def flaky ( lab ):

    result = {}

    for label, retry in (( "no_status_retry", '"connect", "error"' ), ( "retry_5xx", '"connect", "error", "5xx"' )):

        lab.reload(retry = retry)

        before = lab.counters()
        statuses = {}

        for _ in range(300):

            status = lab.get("/api/flaky?percent=30")["status"]
            statuses[status] = statuses.get(status, 0) + 1

        result[label] = {"statuses": statuses, "backends": lab.delta(before, lab.counters())}

    sample = None

    for _ in range(40):

        reply = lab.get("/api/flaky?percent=60")
        trail = lab.journey(reply)

        if trail and any(stage["stage"] == "retry" for stage in trail["stages"]): sample = {"status": reply["status"], **trail}; break

    result["retried_journey"] = sample

    lab.reload(retry = '"connect", "error"')

    return result


def journey ( lab ):

    tokens = lab.shell(lab.options.load, "head -n 3 load/tokens.lua")
    found = re.search(r'tenant = "(\w+)", token = "([^"]+)"', tokens)

    if not found: return {"error": "no token"}

    headers = {"X-Tenant": found.group(1), "Authorization": f"Bearer {found.group(2)}", "Content-Type": "application/json"}
    reply = lab.get("/api/orders", method = "POST", headers = headers, body = '{"lines":[{"item":' + str(4 + int(found.group(1)[1:])) + ',"quantity":1}]}')

    return {"status": reply["status"], "total_ms": round(reply["total_ms"], 2), "server_timing": reply["headers"].get("server-timing"), "journey": lab.journey(reply)}


def stream ( lab ):

    reply = lab.get("/api/events?count=5&ms=200")

    return {"status": reply["status"], "first_byte_ms": round(reply["first_ms"]), "total_ms": round(reply["total_ms"]), "events": reply["body"].count(b"data:")}


def reloads ( lab ):

    before = json.loads(lab.admin("GET", "/state")[1])["config_version"]
    process = lab.wrk(10)
    versions = set()

    time.sleep(1.5)

    for policy in ("least_conn", "round_robin", "least_time", "random", "round_robin"):

        lab.reload(policy = policy)
        versions.add(json.loads(lab.admin("GET", "/state")[1])["config_version"])
        time.sleep(1.2)

    return {**lab.loaded(process), "reloads": 5, "config_versions_seen": len(versions - {before})}


def drain ( lab ):

    process = lab.wrk(8)

    time.sleep(3)

    started = time.monotonic()

    lab.process.send_signal(signal.SIGTERM)

    code = lab.process.wait(timeout = 40)
    took = time.monotonic() - started
    load = lab.loaded(process)

    lab.start()

    return {**load, "exit_code": code, "shutdown_ms": round(took * 1000)}


def upgrade ( lab ):

    process = lab.wrk(10)

    time.sleep(3)

    old = lab.process
    lab.control += 1
    lab.start()

    time.sleep(1)

    started = time.monotonic()

    old.send_signal(signal.SIGTERM)

    code = old.wait(timeout = 40)
    took = time.monotonic() - started

    return {**lab.loaded(process), "old_exit_code": code, "old_shutdown_ms": round(took * 1000)}


def soak ( lab ):

    pid = lab.process.pid
    process = lab.wrk(lab.options.soak, connections = 128)
    samples = []

    def sample ():

        status = Path(f"/proc/{pid}/status").read_text()
        rss = int(re.search(r"VmRSS:\s+(\d+)", status).group(1))

        return {"rss_mb": round(rss / 1024, 1), "fds": len(os.listdir(f"/proc/{pid}/fd")), "threads": int(re.search(r"Threads:\s+(\d+)", status).group(1))}

    started = time.monotonic()

    while process.poll() is None:

        samples.append({"t": round(time.monotonic() - started), **sample()})
        time.sleep(10)

    load = lab.loaded(process)

    time.sleep(3)

    return {**load, "samples": samples, "after_idle": sample()}


SCENARIOS = {"spread": spread, "skew": skew, "failover": failover, "blackout": blackout, "slow": slow, "flaky": flaky, "journey": journey, "stream": stream, "reloads": reloads, "drain": drain, "upgrade": upgrade, "soak": soak}


def main ():

    parser = argparse.ArgumentParser()
    parser.add_argument("--aegisx", required = True)
    parser.add_argument("--bind", required = True)
    parser.add_argument("--backend", required = True)
    parser.add_argument("--load", required = True)
    parser.add_argument("--workers", type = int, default = 2)
    parser.add_argument("--soak", type = int, default = 90)
    parser.add_argument("--only", nargs = "*", default = list(SCENARIOS))
    parser.add_argument("--output", type = Path, required = True)

    options = parser.parse_args()
    lab = Lab(options)

    lab.shell(options.backend, "pkill -x app; sleep 0.5; true", check = False)

    for port in PORTS: lab.app(port, True)

    lab.start()

    try:

        for name in options.only:

            started = time.monotonic()

            try: lab.report["scenarios"][name] = SCENARIOS[name](lab)
            except Exception as error: lab.report["scenarios"][name] = {"error": repr(error)}

            lab.report["scenarios"][name]["took_s"] = round(time.monotonic() - started, 1)
            options.output.write_text(json.dumps(lab.report, indent = 2, default = str) + "\n")

            print(name, json.dumps(lab.report["scenarios"][name], default = str), flush = True)

    finally:

        lab.stop()
        lab.log.close()

        print((lab.root / "aegisx.log").read_text()[-4000:])


if __name__ == "__main__":

    main()
