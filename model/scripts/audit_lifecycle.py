"""Audit the saved experiment and embedded bundle without retraining or tuning."""
import hashlib
import json
from pathlib import Path

import numpy as np

from aegisx_model.module.lifecycle.corpus import Corpus
from aegisx_model.module.lifecycle.export import Export

root = Path(".")
metadata = json.loads((root / "weights/metadata.json").read_text())
data = Corpus(root / ("runs/v09/data" if metadata["model_version"] == "lifecycle-v9" else "runs/v08/data"))
report = json.loads((root / "weights/report.json").read_text())
assert data.manifest["fingerprint"] == metadata["training_fingerprint"]
assert hashlib.sha256((root / "weights/model.onnx").read_bytes()).hexdigest() == metadata["artifact_sha256"]
partitions = data.split()
for index, mask in enumerate(partitions):
    for other in partitions[index + 1:]:
        assert set(data.groups[mask]).isdisjoint(data.groups[other])
runtime = Export.session(root / "weights/model.onnx")
assert {value.name for value in runtime.get_inputs()} == set(data.arrays)
boundaries = []
for task, mask in {"content": data.origins != "recorded_lab", "journey": data.origins == "recorded_lab"}.items():
    indices = np.flatnonzero(partitions[2] & mask)
    scores = Export.scores(runtime, data, indices, task + "_risk")
    labels = data.labels[indices]
    positive = scores >= metadata["recommended_thresholds"][task]
    expected = report["shipped_tasks"][task]["test"]
    assert int((positive & (labels == 1)).sum()) == expected["true_positives"]
    assert int((positive & (labels == 0)).sum()) == expected["false_positives"]
    for offset in np.argsort(np.abs(scores - metadata["recommended_thresholds"][task]))[:8]:
        batch = data.batch([indices[offset]])
        boundaries.append({"task": task, "threshold": metadata["recommended_thresholds"][task],
                           "score": float(scores[offset]),
                           "input": {name: value[0].reshape(-1).tolist() for name, value in batch.items()}})
(root / "weights/boundary-parity.json").write_text(json.dumps(boundaries, indent=2) + "\n")
print("Artifact SHA, split groups, tensor names and shipped task metrics match.")
