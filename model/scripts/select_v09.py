"""Freeze selection using validation, then open held-out tests once for reporting."""
import json
from pathlib import Path

import numpy as np

from aegisx_model.module.lifecycle.corpus import Corpus
from aegisx_model.module.lifecycle.export import Export
from aegisx_model.module.lifecycle.teacher import Teacher
from aegisx_model.module.metrics import Metrics
from aegisx_model.core import ModelError

root = Path(__file__).resolve().parents[1]
directory = root / "runs/v09"
data = Corpus(directory / "data")
partitions = data.split()
artifacts = {"baseline_v08": root.parent / "tmp/model-v08", "supervised": directory / "attempt-2", "distilled": directory / "attempt-3"}
scores = {}
report = {"selection_basis": "Validation only, 0.1% and 1% FPR budgets; no test or external diagnostic tuning",
          "notice": "Budgets constrain validation empirically, not future production. No zero-error guarantee.",
          "candidates": {}}
for name, path in artifacts.items():
    runtime = Export.session(path / "model.onnx")
    scores[name] = {}
    metrics = {}
    for task, mask in {"content": data.origins != "recorded_lab", "journey": data.origins == "recorded_lab"}.items():
        indices = np.flatnonzero(partitions[1] & mask)
        values = Export.scores(runtime, data, indices, task + "_risk")
        scores[name][task] = {"validation": values, "validation_indices": indices}
        metrics[task] = {}
        for budget in (.001, .01):
            try:
                threshold = Metrics.calibrate(data.labels[indices], values, budget)
                feasible = True
            except ModelError:
                threshold, feasible = 1.0, False
            metrics[task][str(budget)] = Metrics.report(data.labels[indices], values, threshold) | {"budget_feasible": feasible}
    report["candidates"][name] = {"validation": metrics}
    print("VALIDATION", name, json.dumps(metrics), flush=True)

# Prefer low-FPR recall; never select a model merely for a larger parameter count.
def objective ( name ):
    tasks = report["candidates"][name]["validation"]
    return sum((tasks[task][str(budget)]["recall"] if tasks[task][str(budget)]["budget_feasible"] else 0) for task in ("content", "journey") for budget in (.001, .01))
winner = max(("supervised", "distilled"), key=objective)
base = report["candidates"]["baseline_v08"]["validation"]
best = report["candidates"][winner]["validation"]
# Stronger strict-FPR content and journey must not hide a regression at the looser content budget.
eligible = (best["content"]["0.001"]["recall"] >= (base["content"]["0.001"]["recall"] if base["content"]["0.001"]["budget_feasible"] else 0)
            and best["content"]["0.01"]["recall"] >= (base["content"]["0.01"]["recall"] if base["content"]["0.01"]["budget_feasible"] else 0)-.005
            and best["journey"]["0.001"]["recall"] >= (base["journey"]["0.001"]["recall"] if base["journey"]["0.001"]["budget_feasible"] else 0))
report["selected"] = winner if eligible else "baseline_v08"
report["selected_path"] = str(artifacts[report["selected"]])
report["validation_candidate"] = winner
report["selection_frozen_before_test"] = True
(directory / "selection.json").write_text(json.dumps(report, indent=2)+"\n")

# At this point all three training attempts and the deployment candidate are fixed.
for name, path in artifacts.items():
    runtime = Export.session(path / "model.onnx")
    tests = {}
    for task, mask in {"content": data.origins != "recorded_lab", "journey": data.origins == "recorded_lab"}.items():
        indices = np.flatnonzero(partitions[2] & mask)
        values = Export.scores(runtime, data, indices, task + "_risk")
        tests[task] = {budget: Metrics.report(data.labels[indices], values, metrics["threshold"])
                       for budget, metrics in report["candidates"][name]["validation"][task].items()}
        if task == "journey":
            # Original lab and new transaction fixture have different generalization scopes.
            for group, match in (("original", np.char.startswith(data.groups[indices], "lab-family-")),
                                 ("transactions", np.char.startswith(data.groups[indices], "transaction-family-"))):
                if match.any():
                    tests[task][group] = Metrics.report(data.labels[indices][match], values[match],
                                                       report["candidates"][name]["validation"][task]["0.001"]["threshold"])
    report["candidates"][name]["test"] = tests
    print("TEST", name, json.dumps(tests), flush=True)

teacher_report = json.loads((directory / "attempt-1/training-report.json").read_text())
teacher_logits = np.load(directory / "attempt-1/teacher-logits.npy")
teacher_sets = Teacher.select(data)
probability = 1/(1+np.exp(-teacher_logits[teacher_sets[2]]))
report["teacher"] = {"frozen_pretrained_parameters": teacher_report["frozen_pretrained_parameters"],
                    "newly_trained_parameters": teacher_report["newly_trained_parameters"],
                    "validation": teacher_report["validation"], "test": {}, "matched_student_tests": {}}
for budget, value in teacher_report["validation"].items():
    report["teacher"]["test"][budget] = Metrics.report(data.labels[teacher_sets[2]], probability, value["threshold"])
for name, path in artifacts.items():
    runtime = Export.session(path / "model.onnx")
    values = Export.scores(runtime, data, teacher_sets[2], "content_risk")
    report["teacher"]["matched_student_tests"][name] = Metrics.report(
        data.labels[teacher_sets[2]], values, report["candidates"][name]["validation"]["content"]["0.001"]["threshold"])
(directory / "evaluation.json").write_text(json.dumps(report, indent=2)+"\n")
print("SELECTED", report["selected"], flush=True)
