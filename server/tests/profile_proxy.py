"""CPU sampling of a forwarding proxy; profiling numbers are not throughput claims."""
import argparse
import json
import os
from pathlib import Path
import subprocess
import tempfile

from native_benchmark import ROOT, measure, port, ready, stop


def run ( options ):
    options.output.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix="aegisx-profile-") as temp:
        temp = Path(temp)
        backend_path = temp / "backend"
        subprocess.run(["gcc", "-O3", "-Wall", "-Wextra", "-Werror",
                        str(ROOT / "server/tests/fixtures/http_backend.c"), "-o", str(backend_path)], check=True)
        upstream, address = port(), port()
        config = temp / "Aegisx.lua"
        config.write_text(f"""set_listen("127.0.0.1:{address}")
set_upstream("127.0.0.1:{upstream}")
set_store(false)
set_model {{mode="off"}}
set_cache {{decisions=false,responses=false}}
set_limits {{rate_limit_10s=0}}
set_runtime {{threads=8,max_in_flight=16384,upstream_keepalive_capacity=256,write_buffer_bytes=0}}
""")
        processes = []
        with (options.output / "process.log").open("w") as log:
            try:
                backend = subprocess.Popen([str(backend_path), str(upstream), "2"], stdout=log, stderr=log, start_new_session=True)
                processes.append(backend)
                ready(upstream, backend)
                proxy = subprocess.Popen([str(options.binary.resolve()), "--config", str(config)], stdout=log, stderr=log, start_new_session=True)
                processes.append(proxy)
                ready(address, proxy)
                settings = argparse.Namespace(client_threads=2, client_cpus=None, connections=128, seconds=2)
                measure(address, settings)
                record = subprocess.Popen([options.perf, "record", "-e", "cpu-clock:u", "-F", "199",
                                           "--call-graph", "dwarf,4096", "-p", str(proxy.pid),
                                           "-o", str(options.output / "perf.data"), "--", "sleep", "20"],
                                          stdout=log, stderr=log, start_new_session=True)
                processes.append(record)
                settings.seconds = 18
                result = measure(address, settings)
                record.wait(timeout=10)
                if record.returncode: raise RuntimeError("perf record failed; see process.log")
                report = subprocess.run([options.perf, "report", "--stdio", "--no-inline", "--no-children", "-g", "none", "--sort", "symbol", "--percent-limit", "0.5",
                                         "-i", str(options.output / "perf.data")], capture_output=True, text=True, check=True, env={**os.environ, "DEBUGINFOD_URLS": ""})
                (options.output / "report.txt").write_text(report.stdout)
                (options.output / "measurement.json").write_text(json.dumps(result, indent=2)+"\n")
                print(report.stdout[:14000])
            finally:
                for process in reversed(processes): stop(process)


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--binary", type=Path, required=True)
    parser.add_argument("--perf", default="/usr/lib/linux-tools-6.8.0-142/perf")
    parser.add_argument("--output", type=Path, required=True)
    run(parser.parse_args())
