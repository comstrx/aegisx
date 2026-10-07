import argparse
import json
import re
import subprocess
import tempfile
import time
from pathlib import Path

from compare import ROOT, nginx_config, port, ready, stop, tree, wrk


def trace ( pids, listen, connections, seconds ):

    command = ["strace", "-c", "-f", "-S", "calls", *sum([["-p", str(pid)] for pid in pids], [])]
    process = subprocess.Popen(command, stdout = subprocess.PIPE, stderr = subprocess.PIPE, text = True)

    time.sleep(0.5)

    output = subprocess.run(["wrk", "-t2", f"-c{connections}", f"-d{seconds}s", f"http://127.0.0.1:{listen}/"], capture_output = True, text = True).stdout
    requests = int(re.search(r"(\d+) requests in", output).group(1))

    time.sleep(0.2)
    process.send_signal(2)
    _, summary = process.communicate(timeout = 20)

    counts = {}

    for line in summary.splitlines():

        fields = line.split()

        if len(fields) >= 5 and re.match(r"^\d", fields[0]) and fields[-1].isidentifier() and fields[-1] != "total":

            counts[fields[-1]] = int(fields[3])

    return counts, requests


def run ( options ):

    backend = ROOT / "backend/backend"
    report = {"targets": {}}

    with tempfile.TemporaryDirectory(prefix = "aegisx-syscalls-") as directory:

        root = Path(directory)
        upstream = port()
        processes = []

        with (root / "process.log").open("w+") as log:

            def launch ( command ):

                process = subprocess.Popen(command, stdout = log, stderr = log, start_new_session = True)
                processes.append(process)

                return process

            try:

                origin = launch([str(backend), str(upstream), "2"])
                ready(upstream, origin)

                for name in options.targets:

                    listen = port()

                    if name == "nginx":

                        (root / "nginx.conf").write_text(nginx_config(root, options.workers, upstream, listen))
                        process = launch([options.nginx, "-p", str(root), "-c", str(root / "nginx.conf")])

                    else:

                        process = launch([str(options.aegisx), "--listen", f"127.0.0.1:{listen}", "--upstream", f"127.0.0.1:{upstream}", "--workers", str(options.workers), "--log", "warn"])

                    ready(listen, process)
                    time.sleep(0.5)

                    wrk(listen, options.connections, 2, 2)

                    pids = tree(process.pid)
                    counts, requests = trace(pids, listen, options.connections, options.seconds)

                    total = sum(counts.values())
                    report["targets"][name] = {"requests": requests, "syscalls_per_request": round(total / requests, 3),
                                               "by_call": {call: round(count / requests, 3) for call, count in sorted(counts.items(), key = lambda item: -item[1])}}

                    stop(process)

            finally:

                for process in reversed(processes): stop(process)

    options.output.write_text(json.dumps(report, indent = 2) + "\n")

    for name, row in report["targets"].items():

        print(f"\n{name}: {row['syscalls_per_request']} syscalls/request over {row['requests']} requests")

        for call, count in row["by_call"].items():

            if count >= 0.01: print(f"  {call:16} {count:>7.3f}")


if __name__ == "__main__":

    parser = argparse.ArgumentParser()
    parser.add_argument("--aegisx", type = Path, default = ROOT.parent / "server/target/release/aegisx")
    parser.add_argument("--nginx", default = "/usr/sbin/nginx")
    parser.add_argument("--targets", default = "nginx,aegisx")
    parser.add_argument("--workers", type = int, default = 4)
    parser.add_argument("--connections", type = int, default = 64)
    parser.add_argument("--seconds", type = int, default = 5)
    parser.add_argument("--output", type = Path, default = ROOT / "results/syscalls.json")
    options = parser.parse_args()
    options.targets = [name.strip() for name in options.targets.split(",") if name.strip()]
    run(options)
