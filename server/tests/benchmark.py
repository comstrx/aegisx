"""Local loopback comparison, not a capacity or production benchmark."""

import argparse
import asyncio
import hashlib
import json
from pathlib import Path
import signal
import socket
import statistics
import sqlite3
import subprocess
import tempfile
import time


def port ():
    with socket.socket() as listener:
        listener.bind(("127.0.0.1", 0))
        return listener.getsockname()[1]


async def backend ( reader, writer ):
    try:
        while True:
            await reader.readuntil(b"\r\n\r\n")
            writer.write(b"HTTP/1.1 200 OK\r\nContent-Length: 128\r\n\r\n" + b"x" * 128)
            await writer.drain()
    except (asyncio.IncompleteReadError, ConnectionError): pass
    finally:
        writer.close()


async def load ( address, requests, concurrency ):
    latencies = []
    async def client ( count ):
        reader, writer = await asyncio.open_connection("127.0.0.1", address)
        try:
            for _ in range(count):
                start = time.perf_counter()
                writer.write(b"GET / HTTP/1.1\r\nHost: benchmark.local\r\n\r\n")
                await writer.drain()
                header = await reader.readuntil(b"\r\n\r\n")
                if not header.startswith(b"HTTP/1.1 200 "): raise RuntimeError(header.decode())
                await reader.readexactly(128)
                latencies.append((time.perf_counter() - start) * 1000)
        finally:
            writer.close()
            await writer.wait_closed()
    start = time.perf_counter()
    await asyncio.gather(*(client(requests // concurrency + int(index < requests % concurrency)) for index in range(concurrency)))
    duration = time.perf_counter() - start
    ordered = sorted(latencies)
    return {"requests": len(latencies), "duration_seconds": duration, "requests_per_second": len(latencies) / duration,
            "p50_ms": statistics.median(latencies), "p95_ms": ordered[int(len(ordered) * 0.95)],
            "p99_ms": ordered[int(len(ordered) * 0.99)]}


async def run ( binary, upstream, capture, requests, concurrency, extra="" ):
    with tempfile.TemporaryDirectory(prefix="aegisx-benchmark-") as temporary:
        root = Path(temporary)
        address = port()
        config = root / "bench.lua"
        config.write_text(
            f'set_listen("127.0.0.1:{address}")\nset_upstream("127.0.0.1:{upstream}")\n'
            'set_model("off")\n'
            f'set_store({json.dumps(str(root / "events.db")) if capture else "false"})\n'
            'set_limits {queue_capacity=65536, retention_events=1000000}\n' + extra
        )
        with (root / "proxy.log").open("w+") as log:
            process = subprocess.Popen([str(binary), "--config", str(config)], stdout=log, stderr=log)
            try:
                for _ in range(200):
                    if process.poll() is not None:
                        log.seek(0)
                        raise RuntimeError(log.read())
                    try:
                        _, writer = await asyncio.open_connection("127.0.0.1", address)
                        writer.close()
                        await writer.wait_closed()
                        break
                    except OSError: await asyncio.sleep(0.025)
                else: raise RuntimeError("Startup timeout")
                await load(address, 512, concurrency)
                result = await load(address, requests, concurrency)
            finally:
                drain_start=time.perf_counter()
                process.send_signal(signal.SIGTERM)
                await asyncio.to_thread(process.wait, 30)
                if "result" in locals(): result["shutdown_drain_ms"]=(time.perf_counter()-drain_start)*1000
            if process.returncode != 0: raise RuntimeError("Proxy failed")
            log.seek(0)
            messages = [json.loads(line) for line in log if line.startswith("{")]
            result["dropped_events"] = next(
                (message["fields"]["dropped_events"] for message in reversed(messages) if "dropped_events" in message.get("fields", {})), None,
            )
        if capture:
            with sqlite3.connect(root/"events.db") as database:
                result["persisted_events"]=database.execute("SELECT COUNT(*) FROM events").fetchone()[0]
                result["persisted_terminal_requests"]=database.execute("SELECT COUNT(DISTINCT request_id) FROM events WHERE stage='completed'").fetchone()[0]
            result["expected_captured_requests_including_warmup"]=requests+512
        return result


async def main ( options ):
    binaries = {"baseline": options.baseline.resolve(), "current": options.current.resolve()}
    server = await asyncio.start_server(backend, "127.0.0.1", 0)
    upstream = server.sockets[0].getsockname()[1]
    report = {
        "scope": "WSL Linux loopback, Python asyncio client and backend share host; 128-byte HTTP/1.1 responses; model off",
        "concurrency": options.concurrency, "requests_per_trial": options.requests,
        "artifacts": {name: {"path": str(path), "sha256": hashlib.sha256(path.read_bytes()).hexdigest()} for name, path in binaries.items()},
        "trials": [],
    }
    try:
        for capture in (False, True):
            for trial in range(options.trials):
                for name in (("baseline", "current") if trial % 2 == 0 else ("current", "baseline")):
                    result = await run(binaries[name], upstream, capture, options.requests, options.concurrency)
                    result.update({"binary": name, "capture": capture, "trial": trial + 1})
                    report["trials"].append(result)
                    print(json.dumps(result), flush=True)
    finally:
        server.close()
        await server.wait_closed()
    report["summary"] = {}
    for capture in (False, True):
        summary = {}
        for name in binaries:
            trials = [trial for trial in report["trials"] if trial["binary"] == name and trial["capture"] == capture]
            summary[name] = {metric: statistics.median(trial[metric] for trial in trials)
                             for metric in ("requests_per_second", "p50_ms", "p95_ms", "p99_ms")}
        summary["throughput_change_percent"] = (summary["current"]["requests_per_second"] / summary["baseline"]["requests_per_second"] - 1) * 100
        report["summary"]["capture" if capture else "proxy_only"] = summary
    options.output.parent.mkdir(parents=True, exist_ok=True)
    options.output.write_text(json.dumps(report, indent=2) + "\n")


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--baseline", type=Path, required=True)
    parser.add_argument("--current", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--requests", type=int, default=6000)
    parser.add_argument("--concurrency", type=int, default=16)
    parser.add_argument("--trials", type=int, default=3)
    asyncio.run(main(parser.parse_args()))
