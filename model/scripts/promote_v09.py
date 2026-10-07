"""Install the validation-selected research student and its complete evidence."""
import hashlib
import json
import shutil
from pathlib import Path

root = Path(__file__).resolve().parents[1]
directory = root / "runs/v09"
selection = json.loads((directory / "evaluation.json").read_text())
ledger = json.loads((directory / "attempts.json").read_text())
if len(ledger["attempts"]) != 3 or any(item["status"] != "completed" for item in ledger["attempts"]):
    raise SystemExit("Three completed candidates are required")
if selection["selected"] == "baseline_v08": raise SystemExit("Validation did not justify replacing the baseline")
candidate = Path(selection["selected_path"])
metadata = json.loads((candidate / "metadata.json").read_text())
if hashlib.sha256((candidate / "model.onnx").read_bytes()).hexdigest() != metadata["artifact_sha256"]:
    raise SystemExit("Candidate artifact changed")
report = json.loads((candidate / "report.json").read_text())
metrics = selection["candidates"][selection["selected"]]
for task in ("content", "journey"):
    report["shipped_tasks"][task]["test"] = metrics["test"][task]["0.001"]
report["round_evaluation"] = selection
teacher = json.loads((directory / "attempt-1/training-report.json").read_text())
metadata["teacher"] = {key: teacher[key] for key in ("frozen_pretrained_parameters", "newly_trained_parameters", "total_parameters")}
metadata["teacher"].update({"repository": "microsoft/codebert-base", "revision": "3b0952feddeffad0063f274080e3c23d75e7eb39",
    "role": "training-only request-content teacher; not shipped or executed by Rust",
    "checkpoint_sha256": "28b61fd8fa069f6bc966f4cb9572a4026ab2a784fca8fb224020d91b744e32d6"})
metadata["calibration"] = {"validation_fpr_budget": .001, "alternative_1pct_thresholds": {
    task: metrics["validation"][task]["0.01"]["threshold"] for task in ("content", "journey")},
    "notice": "Validation budgets are not future-traffic guarantees. Configure each signal explicitly in Lua."}
metadata["evaluation_notice"] = "Research-only compact distilled model. Grouped public-content and controlled SQLite journeys improve; external CRS attack coverage remains low. No general fraud, malware or production safety claim."
for name in ("model.onnx", "parity.json", "features.json", "input-schema.json"):
    shutil.copy2(candidate / name, root / "weights" / name)
for name, value in (("metadata.json", metadata), ("report.json", report)):
    (root / "weights" / name).write_text(json.dumps(value, indent=2)+"\n")
shutil.copy2(candidate / "input-schema.json", root / "src/aegisx_model/lifecycle.json")
for name in ("evaluation.json", "external-evaluation.json", "teacher-ablation.json"):
    shutil.copy2(directory / name, root / "weights" / ("v09-" + name))
notice = ("Training-only teacher: Microsoft CodeBERT, pinned HF revision 3b0952feddeffad0063f274080e3c23d75e7eb39.\n"
          "The embedded compact student was independently trained with original labels and teacher soft targets.\n"
          "No CodeBERT encoder weights or Transformers runtime are embedded.\n\n")
notice += (root / "data/codebert/LICENSE").read_text() + "\n\n" + (root / "data/codebert/NOTICE.md").read_text()
(root / "weights/CODEBERT-NOTICE.txt").write_text(notice)
print(json.dumps({"selected": selection["selected"], "parameters": metadata["parameter_count"],
                  "precision": metadata["precision"], "bytes": (root / "weights/model.onnx").stat().st_size,
                  "sha256": metadata["artifact_sha256"], "deployment_ready": metadata["deployment_ready"]}))
