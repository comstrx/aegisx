"""Verify public sources and record isolated lab journeys; never upload data."""
import argparse
import json
from pathlib import Path

from aegisx_model.module.lifecycle.corpus import Corpus
from aegisx_model.module.lifecycle.lab import Lab

parser = argparse.ArgumentParser()
parser.add_argument("--output", type=Path, default=Path("runs/v08/data"))
args = parser.parse_args()
root, output = Path.cwd(), args.output
lab_path = output.parent / "recorded-lab.jsonl"
if output.exists() or lab_path.exists():
    raise SystemExit("Prepared data already exists; preserve it with its checkpoint and report.")
records, provenance = Corpus.public(root)
lab = Lab.collect(4096)
output.parent.mkdir(parents=True, exist_ok=True)
lab_path.write_text("".join(json.dumps(row) + "\n" for row in lab))
print("Public rows", len(records), "recorded lab", len(lab), flush=True)
report = Corpus.build(records + lab, output, provenance)
data = Corpus(output)
report["splits"] = {name: {"rows": int(mask.sum()), "positive": int(data.labels[mask].sum()),
                         "groups": len(set(data.groups[mask]))}
                    for name, mask in zip(("train", "validation", "test"), data.split(), strict=True)}
(output / "manifest.json").write_text(json.dumps(report, indent=2) + "\n")
print(json.dumps({key: value for key, value in report.items() if key != "provenance"}, indent=2), flush=True)
