"""Three additional user-authorized attempts; fixed validation-only comparison."""
import json
import shutil
import time
from pathlib import Path

import numpy as np

from aegisx_model.module.sources import Sources
from aegisx_model.module.options import TrainingOptions
from aegisx_model.module.train import Trainer
from aegisx_model.module.compress import Compression
from aegisx_model.module.metrics import Metrics

root = Path("runs/v07")
ledger = root / "attempts.json"
state = json.loads(ledger.read_text())
if state["budget"] != 3 or len(state["attempts"]) != 3 or any(item["status"] != "finished" for item in state["attempts"]):
    raise SystemExit("Additional round requires exactly three completed original attempts.")
state["initial_selection"] = state["selected_attempt"]
state["initial_seconds"] = state["total_seconds"]
state["budget"] = 6
state["additional_authorization"] = "Three additional training attempts explicitly authorized by the user on 2026-10-02."
state["comparison_fpr"] = .001
ledger.write_text(json.dumps(state, indent=2) + "\n")
start = time.perf_counter()
data = Sources.http_params(Path("data/sources/payload_full.csv"), Path("data/fuzzdb"),
                           Path("data/wcp"), augment=True, security=Path("data/seclists"))
experiments = [
    dict(learning_rate=.0005, weight_decay=.02, source_balance=False, selection="loss"),
    dict(learning_rate=.00025, weight_decay=.05, source_balance=True, selection="recall"),
    dict(learning_rate=.0002, weight_decay=.01, source_balance=False, selection="recall"),
]
for number, settings in enumerate(experiments, 4):
    state = json.loads(ledger.read_text())
    if len(state["attempts"]) >= 6: raise SystemExit("Six-attempt training budget exhausted")
    state["attempts"].append({"number": number, "settings": settings, "status": "running", "max_validation_fpr": .001})
    ledger.write_text(json.dumps(state, indent=2) + "\n")
    artifact = root / f"attempt-{number}"
    clock = time.perf_counter()
    report = Trainer().run(data, artifact, epochs=80, seed=42, max_fpr=.001,
                           options=TrainingOptions(batch_size=256, balance=True, patience=12,
                                                   checkpoint_every=5, **settings))
    result = {**state["attempts"][-1], "status": "finished", "seconds": time.perf_counter() - clock,
              "epochs": report["epochs"], "best_epoch": report["best_epoch"],
              "validation": report["validation"], "artifact": str(artifact)}
    state["attempts"][-1] = result
    ledger.write_text(json.dumps(state, indent=2) + "\n")
    print("ATTEMPT", json.dumps(result), flush=True)

# Re-evaluate all six floating-point candidates at the SAME validation FPR budget.
# No numeric threshold, checkpoint or winner selection reads test/external labels.
_, validation, test = data.split(42)
comparisons = []
for attempt in state["attempts"]:
    directory = Path(attempt["artifact"])
    source = directory / ("model.fp32.onnx" if (directory / "model.fp32.onnx").exists() else "model.onnx")
    runtime = Compression.runtime(source)
    scores = Compression.scores(runtime, data.values[validation])
    threshold = Metrics.calibrate(data.labels[validation], scores, .001)
    origins, truth = np.asarray(data.origins)[validation], data.labels[validation].reshape(-1)
    recall = [float((scores[(origins == name) & (truth == 1)] >= threshold).mean()) for name in np.unique(origins[truth == 1])]
    comparisons.append({"number": attempt["number"], "threshold": threshold,
                        "macro_attack_recall": float(np.mean(recall)),
                        "validation": Metrics.report(truth, scores, threshold)})
winner = max(comparisons, key=lambda row: (row["macro_attack_recall"], row["validation"]["recall"], -row["number"]))
selected = root / "selected"
selected.mkdir(exist_ok=False)
original = root / f"attempt-{winner['number']}"
for name in ("features.json", "source-signatures.json"):
    shutil.copy2(original / name, selected / name)
for name in ("model.onnx", "metadata.json", "report.json", "parity.json"):
    stem, suffix = name.rsplit(".", 1)
    fp32 = original / f"{stem}.fp32.{suffix}"
    shutil.copy2(fp32 if fp32.exists() else original / name, selected / name)
metadata = json.loads((selected / "metadata.json").read_text())
report = json.loads((selected / "report.json").read_text())
metadata["recommended_threshold"] = winner["threshold"]
report["original_validation_fpr_budget"] = report["max_validation_fpr"]
report.update(metadata)
report["max_validation_fpr"] = .001
runtime = Compression.runtime(selected / "model.onnx")
for name, mask in (("validation", validation), ("test", test)):
    scores = Compression.scores(runtime, data.values[mask])
    origins = np.asarray(data.origins)[mask]
    report[name] = Metrics.report(data.labels[mask], scores, winner["threshold"])
    report[name + "_by_source"] = {origin: Metrics.report(data.labels[mask][origins == origin], scores[origins == origin], winner["threshold"]) for origin in np.unique(origins)}
(selected / "metadata.json").write_text(json.dumps(metadata, indent=2) + "\n")
(selected / "report.json").write_text(json.dumps(report, indent=2) + "\n")
compression = Compression.run(data, selected)
state.update({"selected_attempt": winner["number"], "comparison": comparisons,
              "selection": "All six FP32 candidates compared by macro attack-origin validation recall at FPR <=0.1%; ties use global recall then earlier attempt. Previously inspected diagnostics are not pristine acceptance sets.",
              "additional_seconds": time.perf_counter() - start,
              "compression_promoted": compression["promoted"]})
state["total_seconds"] = state["initial_seconds"] + state["additional_seconds"]
ledger.write_text(json.dumps(state, indent=2) + "\n")
for file in selected.iterdir():
    if file.is_file() and file.suffix != ".pt": shutil.copy2(file, Path("weights") / file.name)
(Path("weights") / "training-round.json").write_text(json.dumps(state, indent=2) + "\n")
print("ROUND_COMPLETE", json.dumps(state), flush=True)
