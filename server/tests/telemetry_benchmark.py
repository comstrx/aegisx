"""Measure the explicit coverage/work tradeoff of full, summary and sampled capture."""

import argparse
import asyncio
import hashlib
import json
from pathlib import Path
import statistics

from benchmark import backend, run


async def main ( options ):

    binary = options.binary.resolve()
    modes = {
        "full": 'set_telemetry { capture="full" }',
        "summary": 'set_telemetry { capture="summary" }',
        "summary_sample_10": 'set_telemetry { capture="summary", sample_every=10 }',
    }
    server = await asyncio.start_server(backend, "127.0.0.1", 0)
    upstream = server.sockets[0].getsockname()[1]
    report = {"binary_sha256":hashlib.sha256(binary.read_bytes()).hexdigest(), "requests":options.requests,
              "concurrency":16, "warmup":512, "scope":"Same WSL host; model off; modes have deliberately different capture coverage", "trials":[]}
    try:
        for trial in range(3):
            for name in (list(modes) if trial % 2 == 0 else list(reversed(modes))):
                result = await run(binary, upstream, True, options.requests, 16, modes[name])
                result.update({"mode":name,"trial":trial+1})
                report["trials"].append(result)
                print(json.dumps(result),flush=True)
    finally:
        server.close()
        await server.wait_closed()
    report["median"] = {name:{key:statistics.median(row[key] for row in report["trials"] if row["mode"] == name)
                             for key in ("requests_per_second","p95_ms","dropped_events")} for name in modes}
    options.output.write_text(json.dumps(report,indent=2)+"\n")


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--binary",type=Path,required=True)
    parser.add_argument("--output",type=Path,required=True)
    parser.add_argument("--requests",type=int,default=20000)
    asyncio.run(main(parser.parse_args()))
