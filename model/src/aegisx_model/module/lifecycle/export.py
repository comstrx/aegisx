"""Export the complete multi-input model; calibrate each task in its own domain."""
import hashlib
import json
import shutil
import time
from pathlib import Path

import numpy as np
import onnx
import onnxruntime as ort
from onnxruntime.quantization import QuantType, quantize_dynamic
import torch

from ..features import Features
from ..metrics import Metrics
from .corpus import Corpus
from .network import LifecycleNetwork


class Export:
    @staticmethod
    def session ( path ):
        options = ort.SessionOptions()
        options.intra_op_num_threads = options.inter_op_num_threads = 1
        return ort.InferenceSession(str(path), sess_options=options, providers=["CPUExecutionProvider"])

    @staticmethod
    def scores ( runtime, data, indices, output ):
        results = []
        for index in indices:
            results.append(float(runtime.run([output], data.batch([index]))[0].item()))
        return np.asarray(results, dtype=np.float32)

    @staticmethod
    def write ( directory, output, evaluate_test=True, update_contract=True ):
        torch.set_num_threads(2)
        data = Corpus(directory)
        selected = torch.load(output / "selected.pt", map_location="cpu", weights_only=True)
        model = LifecycleNetwork(variant=selected.get("variant", "legacy")).eval()
        model.load_state_dict(selected["model"])
        spec = model.spec | {"parameter_count": model.parameters_count()}
        # Keep one shared contract for Python, Rust build constants and metadata.
        if update_contract: Path("src/aegisx_model/lifecycle.json").write_text(json.dumps(spec, indent=2) + "\n")
        sample = {name: torch.from_numpy(value) for name, value in data.batch([0]).items()}
        fp32 = output / "model.fp32.onnx"
        torch.onnx.export(model, tuple(sample.values()), str(fp32), input_names=list(sample),
                          output_names=["risk", "content_risk", "journey_risk"],
                          opset_version=18, dynamo=True, external_data=False)
        graph = onnx.load(fp32)
        # Export optimization can leave stale intermediate transpose shapes.
        # Recompute metadata; keep every operation and initializer unchanged.
        del graph.graph.value_info[:]
        graph = onnx.shape_inference.infer_shapes(graph, strict_mode=True)
        onnx.checker.check_model(graph)
        onnx.save(graph, fp32)
        float_runtime = Export.session(fp32)
        train, validation, test = data.split()
        fixture_indices = np.r_[np.flatnonzero(validation & (data.origins != "recorded_lab"))[:12],
                                np.flatnonzero(validation & (data.origins == "recorded_lab"))[:20]]
        max_error = 0.0
        for index in fixture_indices:
            batch = data.batch([index])
            with torch.no_grad(): expected = [float(value.item()) for value in model(*(torch.from_numpy(x) for x in batch.values()))]
            actual = [float(value.item()) for value in float_runtime.run(None, batch)]
            max_error = max(max_error, float(np.max(np.abs(np.asarray(expected) - actual))))
        if max_error > 1e-5: raise ValueError(f"PyTorch/ONNX mismatch {max_error}")
        quantized = output / "model.uint8.onnx"
        quantize_dynamic(str(fp32), str(quantized), weight_type=QuantType.QUInt8,
                         op_types_to_quantize=["MatMul", "Gemm"], extra_options={"MatMulConstBOnly": True})
        integer_runtime = Export.session(quantized)
        tasks = {"content": data.origins != "recorded_lab", "journey": data.origins == "recorded_lab"}
        quality = {}
        budget = .001 if model.variant == "compact" else .01
        acceptable = True
        for task, mask in tasks.items():
            indices = np.flatnonzero(validation & mask)
            labels = data.labels[indices]
            quality[task] = {}
            for name, runtime in (("fp32", float_runtime), ("uint8", integer_runtime)):
                scores = Export.scores(runtime, data, indices, task + "_risk")
                threshold = Metrics.calibrate(labels, scores, budget)
                quality[task][name] = Metrics.report(labels, scores, threshold)
            acceptable &= quality[task]["uint8"]["recall"] >= quality[task]["fp32"]["recall"] - .005
            print("CALIBRATION", task, quality[task], flush=True)
        samples = [data.batch([index]) for index in fixture_indices]
        timing = {"fp32": [], "uint8": []}
        for _ in range(3):
            for name, runtime in (("fp32", float_runtime), ("uint8", integer_runtime)):
                for batch in samples[:8]: runtime.run(None, batch)
                start = time.perf_counter()
                for index in range(128): runtime.run(None, samples[index % len(samples)])
                timing[name].append((time.perf_counter() - start) * 1e6 / 128)
        acceptable &= quantized.stat().st_size < fp32.stat().st_size * .7
        acceptable &= np.median(timing["uint8"]) < np.median(timing["fp32"])
        precision = "uint8" if acceptable else "fp32"
        path, runtime = (quantized, integer_runtime) if acceptable else (fp32, float_runtime)
        shutil.copy2(path, output / "model.onnx")
        report = json.loads((output / "training-report.json").read_text())
        report["export"] = {"precision": precision, "validation_fpr_budget": budget, "fp32_bytes": fp32.stat().st_size,
                            "uint8_bytes": quantized.stat().st_size, "max_onnx_error": max_error,
                            "timing_us": timing, "validation": quality,
                            "promotion_rule": "At least 30% smaller, lower median latency, at most .005 validation recall loss per task; thresholds refit on validation."}
        report["shipped_tasks"] = {}
        for task, mask in tasks.items():
            indices = np.flatnonzero(test & mask)
            threshold = quality[task][precision]["threshold"]
            report["shipped_tasks"][task] = {"validation": quality[task][precision],
                **({"test": Metrics.report(data.labels[indices], Export.scores(runtime, data, indices, task + "_risk"), threshold)} if evaluate_test else {})}
        fixtures = []
        for index in fixture_indices:
            batch = data.batch([index])
            scores = [float(x.item()) for x in runtime.run(None, batch)]
            fixtures.append({**{name: value[0].reshape(-1).tolist() for name, value in batch.items()},
                             "score": scores[0], "content_score": scores[1], "journey_score": scores[2]})
        metadata = {"model_version": "lifecycle-v9" if model.variant == "compact" else "lifecycle-v8", "input_schema": spec["name"], "feature_version": Features().version,
                    "parameter_count": model.parameters_count(), "architecture": spec, "precision": precision,
                    "artifact_sha256": hashlib.sha256(path.read_bytes()).hexdigest(), "source": "lifecycle_corpus",
                    "deployment_ready": False, "supported_features": model.feature_mask.tolist(),
                    "recommended_thresholds": {task: quality[task][precision]["threshold"] for task in tasks},
                    "evaluation_notice": "Research model: public content labels and controlled SQLite lab journey outcomes are separate tasks. No production middleware, fraud or infrastructure intelligence is established.",
                    "purpose": "Bounded request/response byte sequences, ordered instrumented backend events and numeric context",
                    "training_fingerprint": data.manifest["fingerprint"],
                    "max_onnx_error": max_error, "provenance": data.manifest["provenance"]}
        for name, value in (("metadata.json", metadata), ("report.json", report), ("parity.json", fixtures),
                            ("input-schema.json", spec), ("features.json", Features().spec)):
            (output / name).write_text(json.dumps(value, indent=2) + "\n")
        print("EXPORT_COMPLETE", json.dumps({"parameters": model.parameters_count(), "precision": precision,
              "bytes": path.stat().st_size, "shipped_tasks": report["shipped_tasks"], "export": report["export"]}), flush=True)
