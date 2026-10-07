"""Alternate the same saved inputs across artifacts; include Python call overhead."""
import argparse
import hashlib
import json
import platform
import statistics
import time
from pathlib import Path

import numpy as np

from aegisx_model.module.lifecycle.export import Export

parser = argparse.ArgumentParser()
parser.add_argument("--candidate", type=Path, default=Path("runs/v09/attempt-3"))
parser.add_argument("--baseline", type=Path, default=Path("../tmp/model-v08"))
parser.add_argument("--output", type=Path, default=Path("weights/runtime-benchmark.json"))
args = parser.parse_args()
fixtures = json.loads((args.candidate / "parity.json").read_text())
shapes = {"features": (1, 296), "text": (1, 2, 1024), "event_text": (1, 32, 64),
          "event_values": (1, 32, 8), "coverage": (1, 4)}
samples = [{name: np.asarray(row[name], dtype=np.int64 if name in ("text", "event_text") else np.float32).reshape(shape)
            for name, shape in shapes.items()} for row in fixtures]
paths = {"baseline_v08": args.baseline / "model.onnx", "fp32_v09": args.candidate / "model.fp32.onnx",
         "uint8_v09": args.candidate / "model.onnx"}
sessions = {name: Export.session(path) for name, path in paths.items()}
timings = {name: [] for name in sessions}
for turn in range(5):
    for name in (list(sessions) if turn % 2 == 0 else list(reversed(sessions))):
        runtime = sessions[name]
        for index in range(32): runtime.run(None, samples[index % len(samples)])
        start = time.perf_counter()
        for index in range(256): runtime.run(None, samples[index % len(samples)])
        timings[name].append((time.perf_counter() - start) * 1e6 / 256)
report = {"scope": "Shared WSL CPU; one intra/inter ORT thread; five alternating 256-call trials after 32 warmups; same 32 saved inputs. Python overhead included; extraction/journal/queue/transport excluded.",
          "platform": platform.platform(), "sha256": {name: hashlib.sha256(path.read_bytes()).hexdigest() for name, path in paths.items()},
          "timing_us": timings, "median_us": {name: statistics.median(values) for name, values in timings.items()}}
args.output.write_text(json.dumps(report, indent=2) + "\n")
print(json.dumps(report, indent=2))
