"""Previously inspected external diagnostics at the same validation FPR budgets."""
import hashlib
import json
from pathlib import Path

import numpy as np

from evaluate_lifecycle import crs
from aegisx_model.module.lifecycle.corpus import Corpus
from aegisx_model.module.lifecycle.export import Export
from aegisx_model.module.lifecycle.inputs import Inputs
from aegisx_model.module.sources import Sources
from aegisx_model.module.wcp import Wcp

root = Path(__file__).resolve().parents[1]
directory = root / "runs/v09"
evaluation = json.loads((directory / "evaluation.json").read_text())
selected = evaluation["selected"]
paths = {"baseline_v08": root.parent / "tmp/model-v08", selected: Path(evaluation["selected_path"])}
sessions = {name: Export.session(path / "model.onnx") for name, path in paths.items()}
public, _ = Corpus.public(root)
known = {record["group"].split(":", 1)[1] for record in public}
benign, digests = Wcp.examples(root / "data/wcp", split="external")
sets = {"wcp_benign": [row[0] for row in benign], "crs_positive": crs(root / "data/crs/tests/regression/tests")}
report = {"notice": "Previously inspected WCP/CRS development diagnostics, not pristine acceptance. Same validation FPR budgets; no external fitting. CRS expected-rule positives are proxies, not verified exploit outcomes.",
          "digests": digests, "artifacts": {}}
schema = Inputs()
for label, payloads in sets.items():
    scores = {name: [] for name in paths}
    unseen = []
    for payload in payloads:
        feed = {name: value[None] for name, value in schema.record({"request": payload}).items()}
        unseen.append(hashlib.sha256(Sources.canonical(payload)).hexdigest() not in known)
        for name, runtime in sessions.items():
            scores[name].append(float(runtime.run(["content_risk"], feed)[0].item()))
    unseen = np.asarray(unseen)
    for name, values in scores.items():
        values = np.asarray(values)
        np.save(directory / f"external-{name}-{label}.npy", values)
        metrics = {}
        thresholds = {budget: result["threshold"] for budget, result in evaluation["candidates"][name]["validation"]["content"].items()}
        thresholds["configured_0.95"] = .95
        for budget, threshold in thresholds.items():
            positive = values >= threshold
            metrics[budget] = {"threshold": threshold, "rows": len(values), "positive": int(positive.sum()),
                               "positive_rate": float(positive.mean()), "source_disjoint_rows": int(unseen.sum()),
                               "source_disjoint_positive_rate": float(positive[unseen].mean())}
        report["artifacts"].setdefault(name, {})[label] = metrics
        print(name, label, json.dumps(metrics), flush=True)
(directory / "external-evaluation.json").write_text(json.dumps(report, indent=2)+"\n")
