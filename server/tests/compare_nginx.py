"""Alternating local Nginx/AegisX/direct-backend comparison. No system service changes."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import resource
import statistics
import subprocess
import tempfile
import time

from native_benchmark import ROOT, measure, port, ready, stop


def resources ( process ):
    ticks, rss, pss = 0, 0, 0
    pending = [process.pid]
    while pending:
        pid = pending.pop()
        root = Path(f"/proc/{pid}")
        fields = (root / "stat").read_text().rsplit(") ", 1)[1].split()
        ticks += int(fields[11]) + int(fields[12])
        rss += int(fields[21]) * os.sysconf("SC_PAGE_SIZE")
        for line in (root / "smaps_rollup").read_text().splitlines():
            if line.startswith("Pss:"): pss += int(line.split()[1])*1024
        pending.extend(map(int, (root / f"task/{pid}/children").read_text().split()))
    return {"cpu_seconds": ticks/os.sysconf("SC_CLK_TCK"), "rss_bytes": rss, "pss_bytes": pss}


def run ( options ):
    options.output.parent.mkdir(parents=True, exist_ok=True)
    report = {"scope": "Shared WSL loopback, native epoll backend, 128-byte body, HTTP/1.1 keepalive. "
                       "No TLS, capture, ML, response caching or security policy. Nginx buffering disabled. "
                       "AegisX retains its correlation header and HTTP admission machinery. Not feature parity or isolated capacity.",
              "host": platform.platform(), "logical_cpus": os.cpu_count(), "settings": vars(options).copy(),
              "nginx_build": subprocess.run([options.nginx, "-V"], capture_output=True, text=True, check=True).stderr,
              "aegisx_sha256": hashlib.sha256(options.binary.read_bytes()).hexdigest(), "trials": []}
    report["settings"] = {k: str(v) if isinstance(v, Path) else v for k, v in report["settings"].items()}
    with tempfile.TemporaryDirectory(prefix="aegisx-nginx-") as directory:
        root = Path(directory)
        fixture = root / "backend"
        subprocess.run(["gcc", "-O3", "-Wall", "-Wextra", "-Werror",
                        str(ROOT / "server/tests/fixtures/http_backend.c"), "-o", str(fixture)], check=True)
        upstream, nginx_port, aegis_port = port(), port(), port()
        (root / "nginx.conf").write_text(f"""
daemon off;
master_process on;
worker_processes {options.workers};
pid {root}/nginx.pid;
error_log {root}/nginx-error.log crit;
events {{ worker_connections 16384; multi_accept on; }}
http {{
    access_log off;
    client_body_temp_path {root}/body;
    proxy_temp_path {root}/proxy;
    keepalive_timeout 60s;
    keepalive_requests 100000;
    upstream origin {{ server 127.0.0.1:{upstream}; keepalive 256; }}
    server {{
        listen 127.0.0.1:{nginx_port} reuseport;
        location / {{
            proxy_http_version 1.1;
            proxy_set_header Connection "";
            proxy_set_header Host 127.0.0.1:{upstream};
            proxy_request_buffering off;
            proxy_buffering off;
            proxy_pass http://origin;
        }}
    }}
}}
""")
        (root / "Aegisx.lua").write_text(f"""
set_listen("127.0.0.1:{aegis_port}")
set_upstream("127.0.0.1:{upstream}")
set_store(false)
set_model {{mode="off"}}
set_runtime {{threads={options.workers},max_in_flight=16384,write_buffer_bytes={options.write_buffer},upstream_keepalive_capacity=256}}
set_cache {{decisions=false,responses=false}}
set_limits {{rate_limit_10s=0}}
""")
        report["nginx_config"] = (root / "nginx.conf").read_text()
        report["aegisx_config"] = (root / "Aegisx.lua").read_text()
        report["nofile"] = resource.getrlimit(resource.RLIMIT_NOFILE)
        processes = []
        with (root / "process.log").open("w+") as log:
            try:
                def launch ( command ):
                    process = subprocess.Popen(command, stdout=log, stderr=log, start_new_session=True)
                    processes.append(process)
                    return process
                backend = launch([str(fixture), str(upstream), "2"])
                ready(upstream, backend)
                subprocess.run([options.nginx, "-t", "-p", str(root), "-c", str(root / "nginx.conf")],
                               stdout=log, stderr=log, check=True)
                nginx = launch([options.nginx, "-p", str(root), "-c", str(root / "nginx.conf")])
                aegis = launch([str(options.binary.resolve()), "--config", str(root / "Aegisx.lua")])
                ready(nginx_port, nginx)
                ready(aegis_port, aegis)
                addresses = {"direct": upstream, "nginx": nginx_port, "aegisx": aegis_port}
                targets = {"direct": backend, "nginx": nginx, "aegisx": aegis}
                if options.baseline:
                    baseline_port = port()
                    baseline_config = root / "Baseline.lua"
                    baseline_config.write_text((root / "Aegisx.lua").read_text().replace(f"write_buffer_bytes={options.write_buffer}", "write_buffer_bytes=0").replace(
                        f'127.0.0.1:{aegis_port}', f'127.0.0.1:{baseline_port}'))
                    baseline = launch([str(options.baseline.resolve()), "--config", str(baseline_config)])
                    ready(baseline_port, baseline)
                    addresses["baseline"] = baseline_port
                    targets["baseline"] = baseline
                    report["baseline_sha256"] = hashlib.sha256(options.baseline.read_bytes()).hexdigest()
                for connections in options.connections:
                    for turn in range(options.trials):
                        names = list(addresses)
                        names = names[turn % len(names):] + names[:turn % len(names)]
                        for name in names:
                            settings = argparse.Namespace(client_threads=2, client_cpus=None,
                                                          connections=connections, seconds=2)
                            measure(addresses[name], settings)
                            settings.seconds = options.seconds
                            before = resources(targets[name])
                            started = time.monotonic()
                            result = {"target": name, "connections": connections, "trial": turn+1,
                                      **measure(addresses[name], settings)}
                            elapsed = time.monotonic()-started
                            after = resources(targets[name])
                            cpu = after["cpu_seconds"]-before["cpu_seconds"]
                            result.update({"cpu_cores": cpu/elapsed, "cpu_us_per_request": cpu*1e6/result["requests"],
                                           "rss_bytes": after["rss_bytes"], "pss_bytes": after["pss_bytes"]})
                            report["trials"].append(result)
                            options.output.write_text(json.dumps(report, indent=2) + "\n")
                            print(json.dumps({k: v for k, v in result.items() if k != "raw"}), flush=True)
                report["summary"] = {str(c): {name: statistics.median(
                    row["requests_per_second"] for row in report["trials"]
                    if row["target"] == name and row["connections"] == c) for name in addresses} for c in options.connections}
                options.output.write_text(json.dumps(report, indent=2) + "\n")
            except Exception:
                log.seek(0)
                print(log.read())
                raise
            finally:
                for process in reversed(processes): stop(process)


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--binary", type=Path, default=ROOT / "dist/aegisx")
    parser.add_argument("--write-buffer", type=int, default=4096)
    parser.add_argument("--baseline", type=Path)
    parser.add_argument("--nginx", default="/usr/sbin/nginx")
    parser.add_argument("--workers", type=int, default=8)
    parser.add_argument("--connections", type=int, nargs="+", default=[128, 256])
    parser.add_argument("--seconds", type=int, default=15)
    parser.add_argument("--trials", type=int, default=3)
    parser.add_argument("--output", type=Path, required=True)
    run(parser.parse_args())
