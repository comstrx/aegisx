import argparse
import hashlib
import json
import os
import platform
import re
import signal
import socket
import statistics
import subprocess
import tempfile
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parent
PAGE = os.sysconf("SC_PAGE_SIZE")
TICK = os.sysconf("SC_CLK_TCK")


def port ():

    with socket.socket() as probe:

        probe.bind(("127.0.0.1", 0))

        return probe.getsockname()[1]


def ready ( address, process, timeout = 10.0 ):

    deadline = time.monotonic() + timeout

    while time.monotonic() < deadline:

        if process is not None and process.poll() is not None: raise RuntimeError(f"process exited early with {process.returncode}")

        try:

            with socket.create_connection((BIND, address), timeout = 0.2): return

        except OSError:

            time.sleep(0.05)

    raise RuntimeError(f"port {address} never became ready")


def stop ( process ):

    if process.poll() is not None: return

    try:

        os.killpg(process.pid, signal.SIGTERM)
        process.wait(timeout = 8)

    except Exception:

        os.killpg(process.pid, signal.SIGKILL)
        process.wait(timeout = 5)


def tree ( pid ):

    pending, found = [pid], []

    while pending:

        current = pending.pop()
        found.append(current)

        try: pending.extend(map(int, Path(f"/proc/{current}/task/{current}/children").read_text().split()))
        except OSError: pass

    return found


def resources ( pid ):

    ticks, user, rss, pss, switches, reads, writes = 0, 0, 0, 0, 0, 0, 0

    for current in tree(pid):

        root = Path(f"/proc/{current}")

        try:

            fields = (root / "stat").read_text().rsplit(") ", 1)[1].split()
            ticks += int(fields[11]) + int(fields[12])
            user += int(fields[11])
            rss += int(fields[21]) * PAGE

            for line in (root / "smaps_rollup").read_text().splitlines():

                if line.startswith("Pss:"): pss += int(line.split()[1]) * 1024

            for line in (root / "io").read_text().splitlines():

                if line.startswith("syscr:"): reads += int(line.split()[1])
                if line.startswith("syscw:"): writes += int(line.split()[1])

            for task in (root / "task").iterdir():

                for line in (task / "status").read_text().splitlines():

                    if line.endswith("ctxt_switches:") or "ctxt_switches:" in line: switches += int(line.split()[1])

        except OSError:

            continue

    return {"cpu_seconds": ticks / TICK, "user_seconds": user / TICK, "rss_bytes": rss, "pss_bytes": pss, "switches": switches, "syscalls": reads + writes}


WRK_HOST = None
SCRIPT = None
CACHE_HEADERS = "ignore"
BIND = "127.0.0.1"
PROTOCOL = "h1"
STREAMS = 10


def remote ( command, check = True ):

    if WRK_HOST: command = ["ssh", "-o", "BatchMode=yes", "-o", "StrictHostKeyChecking=no", WRK_HOST, *command]

    completed = subprocess.run(command, capture_output = True, text = True, check = check)

    return completed.stdout + completed.stderr


def scheme ():

    return "http" if PROTOCOL == "h1" else "https"


def load ( address, connections, seconds, threads ):

    if PROTOCOL == "h3": return h3load(address, connections, seconds)

    return h2load(address, connections, seconds, threads) if PROTOCOL == "h2" else wrk(address, connections, seconds, threads)


def h3load ( address, connections, seconds ):

    output = remote(["./bin/h3load", "-c", str(connections), "-m", str(STREAMS), "-d", str(seconds), f"https://{BIND}:{address}/"], check = False)

    def number ( pattern ):

        found = re.search(pattern, output)

        return float(found.group(1)) if found else None

    return {
        "requests_per_second": number(r"Requests/sec:\s+([\d.]+)"),
        "requests": int(number(r"requests: (\d+)") or 0),
        "p50_ms": number(r"p50_ms: ([\d.]+)"), "p90_ms": number(r"p90_ms: ([\d.]+)"), "p99_ms": number(r"p99_ms: ([\d.]+)"),
        "socket_errors": None,
        "non_2xx": int(number(r"errors: (\d+)") or 0),
        "raw": output,
    }


def h2load ( address, connections, seconds, threads ):

    output = remote(["h2load", f"-c{connections}", f"-t{min(threads, connections)}", f"-m{STREAMS}", f"-D{seconds}", "--npn-list=h2", f"https://{BIND}:{address}/"])

    def number ( pattern ):

        found = re.search(pattern, output)

        return float(found.group(1)) if found else None

    def micros ( text ):

        value, unit = re.match(r"([\d.]+)(us|ms|s)", text).groups()

        return float(value) * {"us": 0.001, "ms": 1.0, "s": 1000.0}[unit]

    timing = re.search(r"time for request:\s+(\S+)\s+(\S+)\s+(\S+)\s+(\S+)", output)
    codes = re.search(r"status codes: (\d+) 2xx, (\d+) 3xx, (\d+) 4xx, (\d+) 5xx", output)
    errors = int(number(r"(\d+) failed") or 0) + int(number(r"(\d+) errored") or 0) + int(number(r"(\d+) timeout") or 0)

    return {
        "requests_per_second": number(r"finished in [\d.]+m?s, ([\d.]+) req/s"),
        "requests": int(number(r"requests: (\d+) total") or 0),
        "p50_ms": micros(timing.group(3)) if timing else None, "p90_ms": None, "p99_ms": micros(timing.group(2)) if timing else None,
        "socket_errors": f"{errors} failed/errored/timeout" if errors else None,
        "non_2xx": (int(codes.group(2)) + int(codes.group(3)) + int(codes.group(4))) if codes else 0,
        "raw": output,
    }


def handshakes ( address, pid, seconds = 3 ):

    rates = {}

    for label, flag in (("full", "-new"), ("resumed", "-reuse")):

        before = resources(pid)
        output = remote(["openssl", "s_time", "-connect", f"{BIND}:{address}", flag, "-time", str(seconds)], check = False)
        after = resources(pid)
        found = re.findall(r"(\d+) connections in (\d+) real seconds", output)
        count, real = (int(found[-1][0]), int(found[-1][1])) if found else (0, seconds)

        rates[f"handshakes_{label}_per_s"] = round(count / max(real, 1), 1)
        rates[f"cpu_us_per_{label}_handshake"] = round((after["cpu_seconds"] - before["cpu_seconds"]) * 1e6 / max(count, 1), 1)

    return rates


def wrk ( address, connections, seconds, threads ):

    output = remote(["wrk", f"-t{threads}", f"-c{connections}", f"-d{seconds}s", "--latency", *(["-s", SCRIPT] if SCRIPT else []), f"{scheme()}://{BIND}:{address}/"])

    def number ( pattern ):

        found = re.search(pattern, output)

        return float(found.group(1)) if found else None

    def latency ( label ):

        found = re.search(rf"^\s+{re.escape(label)}\s+([\d.]+)(us|ms|s)", output, re.M)

        if not found: return None

        scale = {"us": 0.001, "ms": 1.0, "s": 1000.0}[found.group(2)]

        return float(found.group(1)) * scale

    return {
        "requests_per_second": number(r"Requests/sec:\s+([\d.]+)"),
        "requests": int(number(r"(\d+) requests in") or 0),
        "p50_ms": latency("50%"), "p90_ms": latency("90%"), "p99_ms": latency("99%"),
        "socket_errors": re.search(r"Socket errors: (.*)", output).group(1) if "Socket errors" in output else None,
        "non_2xx": int(number(r"Non-2xx or 3xx responses: (\d+)") or 0),
        "raw": output,
    }


def upstream_host ( remote ):

    return remote[0] if remote else "127.0.0.1"


def tls_block ( root ):

    if PROTOCOL == "h1": return ""

    return f"""
        ssl_certificate {root}/cert.pem;
        ssl_certificate_key {root}/key.pem;
        ssl_protocols TLSv1.2 TLSv1.3;
        ssl_session_cache shared:SSL:10m;
        ssl_session_tickets on;
        http2_max_concurrent_streams 256;"""


def nginx_config ( root, workers, upstream, listen, access = False, mode = "proxy", upstream_host = "127.0.0.1", bind = "127.0.0.1", identity = False, tag = "nginx" ):

    forwarded = """
            proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
            proxy_set_header X-Forwarded-Proto $scheme;
            proxy_set_header X-Request-Id $request_id;
            add_header X-Request-Id $request_id;""" if identity else ""

    forced = """
            proxy_ignore_headers Cache-Control Expires Set-Cookie;""" if CACHE_HEADERS == "ignore" else ""

    guarded = """
            proxy_no_cache $http_authorization;
            proxy_cache_bypass $http_authorization;""" if SCRIPT else ""

    location = {
        "proxy": f"""
            proxy_http_version 1.1;
            proxy_set_header Connection "";
            proxy_set_header Host {upstream_host}:{upstream};
            proxy_request_buffering off;
            proxy_buffering off;{forwarded}
            proxy_pass http://origin;""",
        "cache": f"""
            proxy_http_version 1.1;
            proxy_set_header Connection "";
            proxy_set_header Host {upstream_host}:{upstream};
            proxy_cache zone;
            proxy_cache_valid 200 60s;{forced}{guarded}
            proxy_pass http://origin;""",
        "static": f"""
            root {root}/site;
            open_file_cache max=1000 inactive=60s;
            open_file_cache_valid 1s;""",
    }[mode]

    cache = f"proxy_cache_path {root}/cache-{tag} levels=1 keys_zone=zone:16m max_size=128m inactive=60s use_temp_path=off;" if mode == "cache" else ""

    return f"""
daemon off;
master_process on;
worker_processes {workers};
pid {root}/{tag}.pid;
error_log {root}/nginx-error.log crit;
events {{ worker_connections 16384; multi_accept on; }}
http {{
    access_log {f'{root}/access.log combined buffer=64k flush=1s' if access else 'off'};
    client_body_temp_path {root}/body;
    proxy_temp_path {root}/proxy;
    keepalive_timeout 60s;
    keepalive_requests 100000;
    {cache}
    upstream origin {{ server {upstream_host}:{upstream}; keepalive 256; }}
    server {{
        {f"listen {bind}:{listen} quic reuseport;" if PROTOCOL == "h3" else ""}
        listen {bind}:{listen} {"" if PROTOCOL == "h1" else "ssl"} {"http2" if PROTOCOL == "h2" else ""} {"" if PROTOCOL == "h3" else "reuseport"};{tls_block(root)}
        location / {{{location}
        }}
    }}
}}
"""


def prepare_mode ( root, mode ):

    (root / "site").mkdir(exist_ok = True)
    (root / "cache").mkdir(exist_ok = True)
    (root / "site/index.html").write_bytes(b"x" * 128)

    lines = {
        "proxy": [],
        "static": [f'add_route {{ name = "files", path = "/", root = "{root}/site" }}'],
        "cache": ['set_cache { enabled = true, valid_ms = { ["200"] = 60000 }' + (', ignore_headers = { "cache-control", "expires", "set-cookie" }' if CACHE_HEADERS == "ignore" and SCRIPT else "") + ' }'],
    }[mode]

    if PROTOCOL != "h1":

        subprocess.run(["openssl", "req", "-x509", "-newkey", "ec", "-pkeyopt", "ec_paramgen_curve:prime256v1", "-nodes", "-keyout", str(root / "key.pem"), "-out", str(root / "cert.pem"), "-days", "2", "-subj", "/CN=bench", "-addext", f"subjectAltName=IP:{BIND},DNS:localhost"], check = True, capture_output = True)
        lines.append(f'set_tls {{ cert = "{root}/cert.pem", key = "{root}/key.pem" }}')
        lines.append(f'set_server {{ http2 = {"true" if PROTOCOL == "h2" else "false"} }}')

        if PROTOCOL == "h3": lines.append("set_http3 { enabled = true }")

    (root / "aegisx.lua").write_text("\n".join(lines) + "\n")


def mode_args ( root, mode ):

    return [f"--config={root}/aegisx.lua"] if (root / "aegisx.lua").read_text().strip() else []


def run ( options ):

    options.output.parent.mkdir(parents = True, exist_ok = True)

    backend = ROOT / "backend/backend"

    if not backend.exists() and not options.upstream:

        subprocess.run(["gcc", "-O2", str(ROOT / "backend/backend.c"), "-o", str(backend)], check = True)

    report = {
        "scope": f"mode={options.mode} protocol={PROTOCOL}; native epoll backend with 2 workers, 128-byte body, keepalive, GET only; "
                 "capture and policy off. Remote mode runs the load generator and the backend on separate hosts.",
        "host": platform.platform(), "logical_cpus": os.cpu_count(),
        "settings": {key: str(value) if isinstance(value, Path) else value for key, value in vars(options).items()},
        "trials": [],
    }

    with tempfile.TemporaryDirectory(prefix = "aegisx-bench-") as directory:

        root = Path(directory)
        upstream = port()
        processes = []
        addresses, targets = {}, {}
        remote_upstream = None

        if options.upstream:

            host, _, remote_port = options.upstream.rpartition(":")
            remote_upstream = (host, int(remote_port))
            upstream = int(remote_port)

        with (root / "process.log").open("w+") as log:

            def launch ( command, env = None ):

                process = subprocess.Popen(command, stdout = log, stderr = log, start_new_session = True, env = env)
                processes.append(process)

                return process

            try:

                prepare_mode(root, options.mode)

                if remote_upstream:

                    origin = None

                else:

                    origin = launch([str(backend), str(upstream), "2"])
                    ready(upstream, origin)

                for name in options.targets:

                    if name == "direct":

                        if origin is None: raise RuntimeError("direct target needs a local backend")

                        addresses[name], targets[name] = upstream, origin

                    elif name.startswith("nginx"):

                        listen = port()
                        nginx = options.nginx_main if "main" in name else options.nginx
                        (root / "nginx.conf").write_text(nginx_config(root, options.workers, upstream, listen, access = name.endswith("log"), mode = options.mode, upstream_host = upstream_host(remote_upstream), bind = BIND, identity = name.endswith("id"), tag = name))
                        subprocess.run([nginx, "-t", "-p", str(root), "-c", str(root / "nginx.conf")], stdout = log, stderr = log, check = True)
                        process = launch([nginx, "-p", str(root), "-c", str(root / "nginx.conf")])
                        ready(listen, process)
                        addresses[name], targets[name] = listen, process
                        report["nginx_build"] = subprocess.run([nginx, "-V"], capture_output = True, text = True).stderr
                        report["nginx_config"] = (root / "nginx.conf").read_text()

                    elif name.startswith("caddy"):

                        listen = port()
                        site = f"root * {root}/site\n    file_server" if options.mode == "static" else f"reverse_proxy {upstream_host(remote_upstream)}:{upstream}"
                        secure = PROTOCOL != "h1"
                        opening = f"https://{BIND}:{listen}" if secure else f":{listen}"
                        extras = f"default_sni {BIND}\n    " if secure else ""
                        manual = f"tls {root}/cert.pem {root}/key.pem\n    " if secure else ""

                        (root / "Caddyfile").write_text(f"{{\n    admin off\n    auto_https off\n    {extras}\n}}\n{opening} {{\n    bind {BIND}\n    {manual}{site}\n}}\n")
                        process = launch([options.caddy, "run", "--config", str(root / "Caddyfile"), "--adapter", "caddyfile"], env = {**os.environ, "GOMAXPROCS": str(options.workers)})
                        ready(listen, process)
                        addresses[name], targets[name] = listen, process
                        report["caddy_build"] = subprocess.run([options.caddy, "version"], capture_output = True, text = True).stdout.strip()
                        report["caddy_config"] = (root / "Caddyfile").read_text()

                    elif name.startswith("pingora"):

                        listen = port()
                        steal = "nosteal" if name.endswith("nosteal") else "steal"
                        process = launch([str(options.pingora), f"127.0.0.1:{listen}", f"127.0.0.1:{upstream}", str(options.workers), steal])
                        ready(listen, process)
                        addresses[name], targets[name] = listen, process
                        report["pingora_sha256"] = hashlib.sha256(options.pingora.read_bytes()).hexdigest()

                    elif name.startswith("aegisx") or name in options.variants or name in options.binaries:

                        listen = port()
                        binary = Path(options.binaries.get(name, options.aegisx))
                        extra = options.variants.get(name, options.aegisx_args).split() if options.variants.get(name, options.aegisx_args) else []
                        extra = [*extra, *mode_args(root, options.mode)]
                        process = launch([str(binary), "--listen", f"{BIND}:{listen}", "--upstream", f"{upstream_host(remote_upstream)}:{upstream}", "--workers", str(options.workers), "--log", "warn", *extra])
                        ready(listen, process)
                        addresses[name], targets[name] = listen, process
                        report.setdefault("binaries", {})[name] = hashlib.sha256(binary.read_bytes()).hexdigest()

                    else:

                        raise RuntimeError(f"unknown target {name}")

                for connections in options.connections:

                    for turn in range(options.trials):

                        names = list(addresses)
                        names = names[turn % len(names):] + names[:turn % len(names)]

                        for name in names:

                            load(addresses[name], connections, options.warmup, options.threads)

                            before = resources(targets[name].pid)
                            started = time.monotonic()
                            result = {"target": name, "connections": connections, "trial": turn + 1, **load(addresses[name], connections, options.seconds, options.threads)}
                            elapsed = time.monotonic() - started
                            after = resources(targets[name].pid)

                            requests = max(result["requests"], 1)
                            cpu = after["cpu_seconds"] - before["cpu_seconds"]
                            user = after["user_seconds"] - before["user_seconds"]

                            result.update({
                                "cpu_cores": cpu / elapsed,
                                "cpu_us_per_request": cpu * 1e6 / requests,
                                "user_us_per_request": user * 1e6 / requests,
                                "sys_us_per_request": (cpu - user) * 1e6 / requests,
                                "switches_per_request": (after["switches"] - before["switches"]) / requests,
                                "vfs_rw_per_request": (after["syscalls"] - before["syscalls"]) / requests,
                                "rss_bytes": after["rss_bytes"], "pss_bytes": after["pss_bytes"],
                            })

                            if options.handshakes and PROTOCOL != "h1": result.update(handshakes(addresses[name], targets[name].pid))

                            report["trials"].append(result)
                            options.output.write_text(json.dumps(report, indent = 2) + "\n")

                            print(json.dumps({key: value for key, value in result.items() if key != "raw"}), flush = True)

                            time.sleep(options.rest)

            except Exception:

                log.seek(0)
                print(log.read())

                raise

            finally:

                for process in reversed(processes): stop(process)

    summary = {}

    for connections in options.connections:

        summary[str(connections)] = {}

        for name in addresses:

            rows = [row for row in report["trials"] if row["target"] == name and row["connections"] == connections]

            if not rows: continue

            median = lambda key: statistics.median(row[key] for row in rows if row[key] is not None)

            summary[str(connections)][name] = {
                "requests_per_second": round(median("requests_per_second"), 1),
                "p50_ms": round(median("p50_ms"), 3), "p99_ms": round(median("p99_ms"), 3),
                "cpu_us_per_request": round(median("cpu_us_per_request"), 2),
                "user_us_per_request": round(median("user_us_per_request"), 2),
                "sys_us_per_request": round(median("sys_us_per_request"), 2),
                "switches_per_request": round(median("switches_per_request"), 3),
                "vfs_rw_per_request": round(median("vfs_rw_per_request"), 3),
                "pss_mb": round(median("pss_bytes") / 1048576, 1),
                "errors": sum(row["non_2xx"] for row in rows), "trials": len(rows),
                **({key: median(key) for key in ("handshakes_full_per_s", "handshakes_resumed_per_s", "cpu_us_per_full_handshake", "cpu_us_per_resumed_handshake")} if any("handshakes_full_per_s" in row for row in rows) else {}),
            }

    report["summary"] = summary
    options.output.write_text(json.dumps(report, indent = 2) + "\n")

    lines = [f"# {options.output.stem}", "", f"mode={options.mode} protocol={PROTOCOL} workers={options.workers} threads={options.threads} seconds={options.seconds} trials={options.trials} host={report['host']}", ""]

    if PROTOCOL == "h3": lines += [f"h3load (bench/h3load: quinn + h3) with {STREAMS} streams per connection, closed loop", ""]

    if PROTOCOL == "h2": lines += [f"h2load with {STREAMS} streams per connection; p50 column holds the mean request time and p99 the maximum, h2load reports no percentiles.", ""]

    for connections, rows in summary.items():

        shaking = any("handshakes_full_per_s" in row for row in rows.values())
        lines += [f"## {connections} connections", "", "| target | req/s | p50 ms | p99 ms | cpu µs/req | user | sys | ctx sw/req | pss MB | errors |" + (" full hs/s | resumed hs/s | server µs/full hs | server µs/resumed hs |" if shaking else ""), "|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|" + ("---:|---:|---:|---:|" if shaking else "")]

        for name, row in rows.items():

            lines.append(f"| {name} | {row['requests_per_second']} | {row['p50_ms']} | {row['p99_ms']} | {row['cpu_us_per_request']} | {row['user_us_per_request']} | {row['sys_us_per_request']} | {row['switches_per_request']} | {row['pss_mb']} | {row['errors']} |" + (f" {row.get('handshakes_full_per_s')} | {row.get('handshakes_resumed_per_s')} | {row.get('cpu_us_per_full_handshake')} | {row.get('cpu_us_per_resumed_handshake')} |" if shaking else ""))

        lines.append("")

    options.output.with_suffix(".md").write_text("\n".join(lines))
    print("\n".join(lines))


if __name__ == "__main__":

    parser = argparse.ArgumentParser()
    parser.add_argument("--aegisx", type = Path, default = ROOT.parent / "server/target/release/aegisx")
    parser.add_argument("--aegisx-args", default = "")
    parser.add_argument("--variant", action = "append", default = [])
    parser.add_argument("--binary", action = "append", default = [])
    parser.add_argument("--pingora", type = Path, default = ROOT / "pingora/target/release/pingora-bare")
    parser.add_argument("--nginx", default = "/usr/sbin/nginx")
    parser.add_argument("--nginx-main", default = "/usr/sbin/nginx")
    parser.add_argument("--caddy", default = "caddy")
    parser.add_argument("--script", default = None)
    parser.add_argument("--cache-headers", choices = ["ignore", "honor"], default = "ignore")
    parser.add_argument("--targets", default = "direct,nginx,pingora,aegisx")
    parser.add_argument("--mode", choices = ["proxy", "static", "cache"], default = "proxy")
    parser.add_argument("--protocol", choices = ["h1", "h1tls", "h2", "h3"], default = "h1")
    parser.add_argument("--streams", type = int, default = 10)
    parser.add_argument("--handshakes", action = "store_true")
    parser.add_argument("--bind", default = "127.0.0.1")
    parser.add_argument("--upstream", default = None)
    parser.add_argument("--wrk-host", default = None)
    parser.add_argument("--workers", type = int, default = 8)
    parser.add_argument("--threads", type = int, default = 2)
    parser.add_argument("--connections", type = int, nargs = "+", default = [128, 256])
    parser.add_argument("--seconds", type = int, default = 12)
    parser.add_argument("--warmup", type = int, default = 2)
    parser.add_argument("--rest", type = float, default = 1.0)
    parser.add_argument("--trials", type = int, default = 4)
    parser.add_argument("--output", type = Path, required = True)
    options = parser.parse_args()
    options.targets = [name.strip() for name in options.targets.split(",") if name.strip()]
    options.variants = dict(item.split("=", 1) for item in options.variant)
    options.binaries = dict(item.split("=", 1) for item in options.binary)
    BIND = options.bind
    WRK_HOST = options.wrk_host
    SCRIPT = options.script
    CACHE_HEADERS = options.cache_headers
    PROTOCOL = options.protocol
    STREAMS = options.streams
    run(options)
