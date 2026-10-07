"""Compare response caching against the same public origin, with measured origin calls."""

import argparse
import asyncio
import hashlib
import json
from pathlib import Path
import statistics

from benchmark import run


async def main ( options ):

    count = 0
    async def backend ( reader, writer ):
        nonlocal count
        try:
            while True:
                await reader.readuntil(b"\r\n\r\n")
                count += 1
                await asyncio.sleep(options.origin_delay_ms / 1000)
                writer.write(b"HTTP/1.1 200 OK\r\nContent-Length: 128\r\nCache-Control: public, max-age=60\r\n\r\n" + b"x" * 128)
                await writer.drain()
        except (asyncio.IncompleteReadError, ConnectionError):
            pass
        finally:
            writer.close()

    server = await asyncio.start_server(backend, "127.0.0.1", 0)
    upstream = server.sockets[0].getsockname()[1]
    binary = options.binary.resolve()
    report = {
        "scope":"Local WSL, same host load generator and origin, model/capture off, public GET, warmed single key",
        "binary_sha256":hashlib.sha256(binary.read_bytes()).hexdigest(),
        "origin_delay_ms":options.origin_delay_ms, "concurrency":16, "requests_per_trial":options.requests,
        "warmup_requests":512, "trials":[],
    }
    try:
        for trial in range(3):
            for enabled in ((False, True) if trial % 2 == 0 else (True, False)):
                before = count
                extra = f'set_cache {{ responses={str(enabled).lower()}, response_ttl_ms=60000 }}\n'
                result = await run(binary, upstream, False, options.requests, 16, extra)
                result.update({"trial":trial+1,"cache":enabled,"origin_calls_including_warmup":count-before})
                report["trials"].append(result)
                print(json.dumps(result), flush=True)
    finally:
        server.close()
        await server.wait_closed()
    report["median"] = {str(enabled):{
        key:statistics.median(row[key] for row in report["trials"] if row["cache"] == enabled)
        for key in ("requests_per_second","p95_ms","origin_calls_including_warmup")
    } for enabled in (False, True)}
    options.output.write_text(json.dumps(report, indent=2) + "\n")


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--binary", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--requests", type=int, default=10000)
    parser.add_argument("--origin-delay-ms", type=float, default=2)
    asyncio.run(main(parser.parse_args()))
