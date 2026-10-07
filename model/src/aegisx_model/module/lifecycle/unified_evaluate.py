"""Independent calibration and one final held-out evaluation; promotion fails closed."""
import hashlib
import json
import time

import numpy as np
import torch

from ..metrics import Metrics
from ...core import ModelError
from .export import Export
from .prediction import PredictionSet
from .unified import Unified
from .unified_data import UnifiedData
from .unified_train import UnifiedTraining


class UnifiedEvaluation:
    @staticmethod
    def report ( labels, scores, threshold, prediction_set ):
        result = Metrics.report(labels, scores, threshold)
        predicted = PredictionSet.apply(scores, prediction_set)
        accepted = predicted != -1
        result["prediction_sets"] = {"unknown": int((~accepted).sum()), "rows": len(labels),
                                    "classified": int(accepted.sum()),
                                    "errors_among_classified": int((predicted[accepted] != labels[accepted]).sum())}
        result["brier"] = float(np.mean((scores-labels)**2))
        return result

    @staticmethod
    def run ( directory, pretrained, candidate, baseline ):
        if (candidate / "evaluation.json").exists(): raise ValueError("Final evaluation already recorded; do not tune on its test results")
        torch.set_num_threads(2)
        data = UnifiedData(directory)
        checkpoint = torch.load(candidate / "selected.pt", weights_only=True, map_location="cpu", mmap=True)
        if checkpoint["data_fingerprint"] != data.manifest["fingerprint"]: raise ValueError("Selected checkpoint/corpus mismatch")
        validation_failures = [f"task {task}: selection recall {metrics['recall']:.3f} is below the predeclared .90 target"
                               for task, metrics in checkpoint["validation"].items() if metrics["recall"] < .9]
        if validation_failures:
            report = {"fingerprint": data.manifest["fingerprint"], "selected_stage": checkpoint["stage"],
                      "selected_counts": checkpoint["counts"], "validation": checkpoint["validation"],
                      "calibration_and_test_consumed": False,
                      "promotion": {"allowed": False, "failures": validation_failures, "shipped_artifact_unchanged": True},
                      "notice": "Candidate fails the validation prerequisite. Keep calibration and unseen-application test data unopened for model scoring; no production or model-family conclusion follows from this short pilot."}
            (candidate / "assessment.json").write_text(json.dumps(report, indent=2)+"\n")
            return report
        model = Unified.load(pretrained, compact=data.manifest.get("schema_version", 1) >= 2)
        model.load_state_dict(checkpoint["model"])
        model.eval()
        runtime = Export.session(baseline / "model.onnx")
        old = {name: np.load(directory / ("baseline_"+name+".npy"), mmap_mode="r")
               for name in ("features", "text", "event_text", "event_values", "coverage")}
        def previous ( indices, task ):
            result = []
            for index in indices:
                batch = {name: np.asarray(value[index:index+1], dtype=np.int64 if name in ("text", "event_text") else np.float32)
                         for name, value in old.items()}
                result.append(float(runtime.run(["content_risk" if task == 0 else "journey_risk"], batch)[0].item()))
            return np.asarray(result)
        calibrated = {}
        for task in (0, 1):
            indices = np.flatnonzero((data.partitions == 3) & (data.tasks == task))
            labels = data.labels[indices]
            calibrated[str(task)] = {}
            scores = {"candidate": UnifiedTraining.scores(model, data, indices)[:, task],
                      "baseline": previous(indices, task)}
            for name, values in scores.items():
                try: threshold = Metrics.calibrate(labels, values, .01)
                except ModelError: threshold = 1.0
                prediction_set = PredictionSet.fit(labels, values, alpha=.05)
                calibrated[str(task)][name] = {"threshold": threshold, "prediction_set": prediction_set,
                                               "metrics": Metrics.report(labels, values, threshold),
                                               "budget_feasible": Metrics.report(labels, values, threshold)["false_positive_rate"] <= .01}
            print("CALIBRATED", task, flush=True)
        # Freeze thresholds before inspecting any test labels/scores.
        (candidate / "calibration.json").write_text(json.dumps(calibrated, indent=2)+"\n")
        tests = {}
        cohorts = {"public-content": (data.tasks == 0, 0, 192),
                   "legacy-lab": (data.applications == "legacy-lab", 1, 192),
                   "unseen-controlled-apps": (~np.isin(data.applications, ["legacy-lab", "public-content"]), 1, 384)}
        for name, (mask, task, limit) in cohorts.items():
            indices = UnifiedData.stratified(data.labels, np.flatnonzero((data.partitions == 2) & mask), limit, 3300+task)
            labels = data.labels[indices]
            scores = {"candidate": UnifiedTraining.scores(model, data, indices)[:, task],
                      "baseline": previous(indices, task)}
            tests[name] = {"applications": sorted(set(data.applications[indices].tolist())), "rows": len(indices)}
            for engine, values in scores.items():
                policy = calibrated[str(task)][engine]
                tests[name][engine] = UnifiedEvaluation.report(labels, values, policy["threshold"], policy["prediction_set"])
            print("TEST", name, json.dumps(tests[name]), flush=True)
        sample = UnifiedTraining.tensors(data, np.flatnonzero(data.partitions == 2)[:1])
        timings = []
        with torch.inference_mode():
            for _ in range(2): model(**sample)
            for _ in range(8):
                started = time.perf_counter()
                model(**sample)
                timings.append((time.perf_counter()-started)*1000)
        failures = []
        for name, result in tests.items():
            new, old_result = result["candidate"], result["baseline"]
            if new["recall"] < .9 or new["false_positive_rate"] > .01:
                failures.append(name+": below research target of recall >=90% and FPR <=1%")
            if new["recall"] < old_result["recall"] or new["false_positive_rate"] > old_result["false_positive_rate"]:
                failures.append(name+": does not dominate baseline at separately calibrated 1% FPR budgets")
        # Independent deployment proof cannot be inferred from a controlled generator or a PyTorch checkpoint.
        failures.extend(["No independent production-application acceptance set",
                         "New tokenizer/input contract has not passed Rust/ONNX integration and capacity checks"])
        report = {"fingerprint": data.manifest["fingerprint"], "selected_stage": checkpoint["stage"],
                  "selected_counts": checkpoint["counts"], "baseline_sha256": hashlib.sha256((baseline / "model.onnx").read_bytes()).hexdigest(),
                  "calibration": calibrated, "tests": tests, "single_sample_cpu_ms": timings,
                  "promotion": {"allowed": not failures, "failures": failures, "shipped_artifact_unchanged": True},
                  "notice": "One shared backbone, two distinct supervised targets. Prediction-set abstention is not an OOD detector "
                            "or a guarantee under distribution shift. Test subsamples fixed by seed before inspection. "
                            "Controlled unseen applications share fixture logic; public examples come from previously studied sources."}
        (candidate / "evaluation.json").write_text(json.dumps(report, indent=2)+"\n")
        return report
