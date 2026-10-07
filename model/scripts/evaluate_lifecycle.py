"""Fixed-threshold diagnostics for the text/journey artifact; no fitting."""
import argparse
import hashlib
import json
from pathlib import Path

import numpy as np
import yaml

from aegisx_model.module.content import Content
from aegisx_model.module.features import Features
from aegisx_model.module.lifecycle.corpus import Corpus
from aegisx_model.module.lifecycle.export import Export
from aegisx_model.module.lifecycle.inputs import Inputs
from aegisx_model.module.sources import Sources
from aegisx_model.module.wcp import Wcp


def crs ( directory ):
    rows, seen = [], set()
    for path in sorted(directory.rglob("*.yaml")):
        if path.name[:3] not in {"930", "931", "932", "933", "934", "941", "942", "944"}: continue
        for case in yaml.safe_load(path.read_text()).get("tests", []):
            for stage in case.get("stages", []):
                request, output = stage.get("input", {}), stage.get("output", {})
                if not output.get("log", {}).get("expect_ids"): continue
                if any(name in request for name in ("raw_request", "encoded_request")): continue
                uri, body = request.get("uri", "/"), request.get("data", "")
                if not isinstance(uri, str) or not isinstance(body, str): continue
                if uri in {"/", "/get", "/post", "/index.html"} and not body: continue
                payload = (uri + body).encode()
                if payload not in seen:
                    rows.append(payload)
                    seen.add(payload)
    return rows


def evaluate ( directory, sets, known ):
    metadata = json.loads((directory / "metadata.json").read_text())
    runtime = Export.session(directory / "model.onnx")
    inputs = Inputs()
    schema = Features(json.loads((directory / "features.json").read_text()))
    content = Content(schema.spec)
    text_model = bool(metadata.get("input_schema"))
    threshold = metadata["recommended_thresholds"]["content"] if text_model else metadata["recommended_threshold"]
    report = {"sha256": metadata["artifact_sha256"], "parameters": metadata["parameter_count"], "threshold": threshold, "sets": {}}
    for name, payloads in sets.items():
        scores = []
        for payload in payloads:
            if text_model:
                feed = {key: value[None] for key, value in inputs.record({"request": payload}).items()}
                score = runtime.run(["content_risk"], feed)[0].item()
            else:
                feed = {"features": schema.normalize([content.row(payload[:16384], len(payload))])}
                score = runtime.run(["risk"], feed)[0].item()
            scores.append(score)
        positive = np.asarray(scores) >= threshold
        unseen = np.asarray([hashlib.sha256(Sources.canonical(value)).hexdigest() not in known for value in payloads])
        report["sets"][name] = {
            "rows": len(payloads), "positive": int(positive.sum()), "positive_rate": float(positive.mean()),
            "configured_0_95": {"positive": int((np.asarray(scores) >= .95).sum()), "positive_rate": float((np.asarray(scores) >= .95).mean())},
            "source_disjoint": {"rows": int(unseen.sum()), "positive": int(positive[unseen].sum()),
                                "positive_rate": float(positive[unseen].mean()), "excluded": int((~unseen).sum())}}
        print(name, report["sets"][name], flush=True)
    return report


def main ():
    parser = argparse.ArgumentParser()
    parser.add_argument("--artifact", type=Path, default=Path("weights"))
    parser.add_argument("--baseline", type=Path, default=Path("../tmp/model-v07"))
    args = parser.parse_args()
    records, _ = Corpus.public(Path("."))
    known = {record["group"].split(":", 1)[1] for record in records}
    benign, digests = Wcp.examples(Path("data/wcp"), split="external")
    sets = {"wcp_benign": [row[0] for row in benign], "crs_positive": crs(Path("data/crs/tests/regression/tests"))}
    report = {
        "notice": "Previously inspected development diagnostics, not pristine acceptance. WCP publisher benign; CRS expected rule triggers are positive-only proxies. Different artifacts use their own validation thresholds, not equal FPR. Canonical source-disjoint filtering cannot remove all near-duplicates.",
        "digests": digests,
        "artifacts": [evaluate(directory, sets, known) for directory in (args.artifact, args.baseline)]
    }
    (args.artifact / "external-lifecycle-report.json").write_text(json.dumps(report, indent=2) + "\n")


if __name__ == "__main__": main()
