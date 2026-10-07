"""Recompute shipped per-source validation metrics without selecting/tuning anything."""
import argparse
import json
from pathlib import Path

import numpy as np

from aegisx_model.module.compress import Compression
from aegisx_model.module.metrics import Metrics
from aegisx_model.module.sources import Sources

parser = argparse.ArgumentParser()
parser.add_argument("--artifact", type=Path, default=Path("weights"))
args = parser.parse_args()
data = Sources.http_params(Path("data/sources/payload_full.csv"), Path("data/fuzzdb"),
                           Path("data/wcp"), augment=True, security=Path("data/seclists"))
report = json.loads((args.artifact / "report.json").read_text())
if data.fingerprint() != report["training_fingerprint"]:
    raise ValueError("Audit corpus differs from training provenance")
_, validation, _ = data.split(report["seed"])
scores = Compression.scores(Compression.runtime(args.artifact / "model.onnx"), data.values[validation])
labels, origins = data.labels[validation], np.asarray(data.origins)[validation]
threshold = report["recommended_threshold"]
assert Metrics.report(labels, scores, threshold) == report["validation"]
report["validation_by_source"] = {name: Metrics.report(labels[origins == name], scores[origins == name], threshold) for name in np.unique(origins)}
(args.artifact / "report.json").write_text(json.dumps(report, indent=2) + "\n")
print("Artifact validation audit matches; per-source metrics use shipped precision.")
