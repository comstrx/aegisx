"""Fixed-threshold held-out benign captures; no calibration or training."""
import argparse
import hashlib
import json
from pathlib import Path

import numpy as np
import onnxruntime as ort

from aegisx_model.module.content import Content
from aegisx_model.module.features import Features
from aegisx_model.module.wcp import Wcp
from aegisx_model.module.sources import Sources

parser = argparse.ArgumentParser()
parser.add_argument("--source", type=Path, default=Path("data/wcp"))
parser.add_argument("--artifact", type=Path, default=Path("weights"))
parser.add_argument("--baseline", type=Path)
args = parser.parse_args()
examples, digests = Wcp.examples(args.source, split="external")
content, schema = Content(), Features()
raw = [content.row(payload[:16384], len(payload)) for payload, *_ in examples]
values = schema.normalize(raw)
known = set(json.loads((args.artifact / "source-signatures.json").read_text()))
unseen = np.array([
    "t:" + hashlib.sha256(Sources.canonical(example[0])).hexdigest() not in known
    and "f:" + hashlib.sha256(row.astype("<f4").tobytes()).hexdigest() not in known
    for example, row in zip(examples, values, strict=True)
])
report = {
    "source": "OpenAppSec WAF Comparison Project, 16 held-out benign capture files",
    "rows": len(examples), "digests": digests,
    "notice": "Publisher-labeled benign; diagnostic set inspected during development, not pristine acceptance data. URI/body only. Max 512 hash-selected unique payloads per capture. Captures excluded from training and threshold selection; sites and near-duplicates may overlap.",
    "artifacts": []
}
for directory in [args.artifact] + ([args.baseline] if args.baseline else []):
    metadata = json.loads((directory / "metadata.json").read_text())
    options = ort.SessionOptions()
    options.intra_op_num_threads = 1
    runtime = ort.InferenceSession(str(directory / "model.onnx"), sess_options=options, providers=["CPUExecutionProvider"])
    count = runtime.get_inputs()[0].shape[1]
    artifact_schema = json.loads((directory / "features.json").read_text()) if (directory / "features.json").exists() else schema.spec
    if artifact_schema["version"] != metadata["feature_version"]:
        raise ValueError("Artifact-specific feature schema is required; slicing a newer hash vector is invalid")
    extractor = Content(artifact_schema)
    selected = values if artifact_schema == schema.spec else Features(artifact_schema).normalize([extractor.row(payload[:16384], len(payload)) for payload, *_ in examples])
    scores = np.array([runtime.run(["risk"], {"features": row.reshape(1, count)})[0].item() for row in selected])
    positive = scores >= metadata["recommended_threshold"]
    report["artifacts"].append({
        "sha256": metadata["artifact_sha256"], "parameters": metadata["parameter_count"],
        "threshold": metadata["recommended_threshold"], "false_positives": int(positive.sum()),
        "fpr": float(positive.mean()),
        "source_disjoint_relative_to_candidate_corpus": {
            "rows": int(unseen.sum()), "excluded_overlap": int((~unseen).sum()),
            "false_positives": int(positive[unseen].sum()), "fpr": float(positive[unseen].mean())
        }
    })
(args.artifact / "external-benign-report.json").write_text(json.dumps(report, indent=2) + "\n")
print(json.dumps({**report, "digests": f"{len(digests)} verified captures"}, indent=2))
