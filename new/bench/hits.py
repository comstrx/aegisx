#!/usr/bin/env python3

import argparse
import collections
import http.client
import json
import os
import secrets
import subprocess
import tempfile
import time

from pathlib import Path

SSH = ["ssh", "-o", "BatchMode=yes", "-o", "StrictHostKeyChecking=no"]

NGINX = """
daemon off;
worker_processes {workers};
pid {root}/nginx.pid;
error_log {root}/error.log crit;
events {{ worker_connections 16384; multi_accept on; }}
http {{
    log_format cache '$upstream_cache_status $request_method $status';
    access_log {root}/cache.log cache buffer=256k flush=1s;
    client_body_temp_path {root}/body;
    proxy_temp_path {root}/proxy;
    keepalive_requests 100000;
    proxy_cache_path {root}/cache levels=1 keys_zone=zone:16m max_size=128m inactive=60s use_temp_path=off;
    upstream origin {{ server {upstream}; keepalive 256; }}
    server {{
        listen {bind}:{listen} reuseport;
        location / {{
            proxy_http_version 1.1;
            proxy_set_header Connection "";
            proxy_set_header Host {upstream};
            proxy_cache zone;
            proxy_cache_valid 200 60s;
            proxy_no_cache $http_authorization;
            proxy_cache_bypass $http_authorization;
            proxy_pass http://origin;
        }}
    }}
}}
"""

AEGISX = """
set_listen("{bind}:{listen}")
set_upstream("{upstream}")
set_runtime {{ workers = {workers} }}
set_log {{ level = "warn" }}
set_cache {{ enabled = true, valid_ms = {{ ["200"] = 60000 }} }}
set_control {{ enabled = true, listen = "127.0.0.1:{control}", token_env = "AEGISX_HITS_TOKEN" }}
"""


def load ( options, listen ):

    command = f"wrk -t2 -c{options.connections} -d{options.seconds}s --latency -s load/mix.lua http://{options.bind}:{listen}/"

    return subprocess.run([*SSH, f"ubuntu@{options.load}", command], capture_output = True, text = True).stdout


def rate ( output ):

    for line in output.splitlines():

        if line.startswith("Requests/sec:"): return float(line.split()[1])

    return 0.0


def nginx ( options, root ):

    listen = 18180
    config = root / "nginx.conf"

    config.write_text(NGINX.format(root = root, workers = options.workers, upstream = options.upstream, bind = options.bind, listen = listen))

    process = subprocess.Popen([options.nginx, "-p", str(root), "-c", str(config)], stdout = subprocess.DEVNULL, stderr = subprocess.DEVNULL)

    time.sleep(1)
    load(options, listen)

    (root / "cache.log").write_text("")

    output = load(options, listen)

    process.terminate()
    process.wait()

    counts = collections.Counter(line.split()[0] for line in (root / "cache.log").read_text().splitlines() if line)

    return {"rps": rate(output), "statuses": dict(counts)}


def aegisx ( options, root ):

    listen, control = 18181, 18191
    token = secrets.token_hex(32)
    config = root / "aegisx.lua"

    config.write_text(AEGISX.format(workers = options.workers, upstream = options.upstream, bind = options.bind, listen = listen, control = control))

    process = subprocess.Popen([options.aegisx, "--config", str(config)], stdout = subprocess.DEVNULL, stderr = subprocess.DEVNULL, env = {**os.environ, "AEGISX_HITS_TOKEN": token})

    def stats ():

        client = http.client.HTTPConnection("127.0.0.1", control, timeout = 5)
        client.request("GET", "/api/v1/cache", headers = {"Authorization": f"Bearer {token}", "Host": f"127.0.0.1:{control}"})

        return json.loads(client.getresponse().read())

    time.sleep(1)
    load(options, listen)

    before = stats()
    output = load(options, listen)
    after = stats()

    process.terminate()
    process.wait()

    return {"rps": rate(output), "statuses": {name: after[name] - before[name] for name in ("hits", "misses", "stale", "filled")}, "entries": after["entries"], "bytes": after["bytes"]}


def main ():

    parser = argparse.ArgumentParser()
    parser.add_argument("--aegisx", required = True)
    parser.add_argument("--nginx", default = "/usr/sbin/nginx")
    parser.add_argument("--bind", required = True)
    parser.add_argument("--upstream", required = True)
    parser.add_argument("--load", required = True)
    parser.add_argument("--workers", type = int, default = 2)
    parser.add_argument("--connections", type = int, default = 256)
    parser.add_argument("--seconds", type = int, default = 20)

    options = parser.parse_args()
    root = Path(tempfile.mkdtemp(prefix = "hits-"))

    print(json.dumps({"nginx": nginx(options, root), "aegisx": aegisx(options, root)}, indent = 2))


if __name__ == "__main__":

    main()
