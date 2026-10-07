"""Fixed-threshold diagnostic of the frozen teacher's actually used representations."""
import json
from pathlib import Path

import numpy as np
import torch

from aegisx_model.module.lifecycle.corpus import Corpus
from aegisx_model.module.lifecycle.teacher import Teacher
from aegisx_model.module.metrics import Metrics

torch.set_num_threads(2)
root = Path(__file__).resolve().parents[1] / "runs/v09"
data = Corpus(root / "data")
indices = np.load(root / "teacher-cache/indices.npy")
positions = {int(index): slot for slot, index in enumerate(indices)}
test = Teacher.select(data)[2]
slots = [positions[int(index)] for index in test]
features = data.arrays["features"][test]
x = np.concatenate((np.load(root / "teacher-cache/embeddings.npy")[slots], features[:, 16:32], features[:, 40:]), axis=-1)
head = Teacher.head().eval()
head.load_state_dict(torch.load(root / "attempt-1/selected.pt", weights_only=True)["head"])
report = json.loads((root / "attempt-1/training-report.json").read_text())
result = {"notice": "Zero ablation changes input distribution; tests modality dependence, not causal value or production accuracy."}
for kind in ("full", "without_pretrained_embedding", "without_numeric_features"):
    value = x.copy()
    if kind == "without_pretrained_embedding": value[:, :1536] = 0
    if kind == "without_numeric_features": value[:, 1536:] = 0
    with torch.inference_mode():
        scores = torch.sigmoid(head(torch.from_numpy(value))).flatten().numpy()
    result[kind] = {budget: Metrics.report(data.labels[test], scores, metric["threshold"])
                    for budget, metric in report["validation"].items()}
(root / "teacher-ablation.json").write_text(json.dumps(result, indent=2)+"\n")
print(json.dumps(result), flush=True)
