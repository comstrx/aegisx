"""Alternating CPU timings of the saved FP32/UINT8 artifacts; no quality calibration."""
import argparse
import hashlib
import json
import platform
from pathlib import Path

import numpy as np

from aegisx_model.module.compress import Compression

parser = argparse.ArgumentParser()
parser.add_argument("--artifact", type=Path, default=Path("weights"))
parser.add_argument("--output", type=Path, default=Path("weights/runtime-benchmark.json"))
args = parser.parse_args()
fixtures = json.loads((args.artifact / "parity.json").read_text())
values = np.asarray([fixtures[index % len(fixtures)]["features"] for index in range(1000)], dtype=np.float32)
metadata = json.loads((args.artifact / "metadata.json").read_text())
selected = metadata.get("precision", "fp32")
paths = {"fp32": args.artifact / ("model.fp32.onnx" if selected == "uint8" else "model.onnx"),
         "uint8": args.artifact / ("model.onnx" if selected == "uint8" else "model.int8.onnx")}
sessions = {name: Compression.runtime(path) for name, path in paths.items()}
report = {
    "scope": "Shared WSL CPU; ORT CPU one intra/inter thread; 1000 single-row calls, five alternating trials, 100 warmups. Repeats 32 saved reference vectors. Includes Python call/reshape overhead; excludes extraction, journal, queue and networking. No labels or threshold selection.",
    "platform": platform.platform(), "selected_precision": selected,
    "sha256": {name: hashlib.sha256(path.read_bytes()).hexdigest() for name, path in paths.items()},
    "latency": Compression.latency(sessions, values)
}
args.output.parent.mkdir(parents=True, exist_ok=True)
args.output.write_text(json.dumps(report, indent=2) + "\n")
print(json.dumps(report, indent=2))
