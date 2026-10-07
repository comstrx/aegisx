"""Measured dynamic INT8 candidate. Promotion uses validation, never external/test labels."""
import hashlib
import json
import shutil
import statistics
import time

import numpy as np
import onnx
import onnxruntime as ort
from onnxruntime.quantization import QuantType, quantize_dynamic
from onnxruntime.quantization.shape_inference import quant_pre_process

from .metrics import Metrics


class Compression:
    @staticmethod
    def runtime ( path ):
        options = ort.SessionOptions()
        options.intra_op_num_threads = 1
        options.inter_op_num_threads = 1
        return ort.InferenceSession(str(path), sess_options=options, providers=["CPUExecutionProvider"])

    @staticmethod
    def scores ( session, rows ):
        return np.asarray([session.run(["risk"], {"features": row.reshape(1, -1)})[0].item() for row in rows], dtype=np.float32)

    @staticmethod
    def latency ( sessions, values ):
        timings = {name: [] for name in sessions}
        for session in sessions.values(): Compression.scores(session, values[:100])
        for trial in range(5):
            for name in list(sessions) if trial % 2 == 0 else reversed(list(sessions)):
                start = time.perf_counter_ns()
                Compression.scores(sessions[name], values)
                timings[name].append((time.perf_counter_ns() - start) / len(values) / 1000)
        return {name: {"median_us": statistics.median(rows), "trials_us": rows} for name, rows in timings.items()}

    @staticmethod
    def run ( data, output, seed=42 ):
        metadata = json.loads((output / "metadata.json").read_text())
        if data.fingerprint() != metadata["training_fingerprint"]: raise ValueError("Compression requires the exact source corpus")
        if metadata.get("precision", "fp32") != "fp32": raise ValueError("Start from the floating-point artifact")
        source, prepared, target = output / "model.onnx", output / "prepared.onnx", output / "model.int8.onnx"
        quant_pre_process(source, prepared, skip_symbolic_shape=True)
        graph = onnx.load(prepared)
        # Gemm-to-MatMul transposes weight initializers. Drop redundant initializer
        # value_info so ORT's converter does not retain their old transposed shapes.
        initializers = {value.name for value in graph.graph.initializer}
        retained = [value for value in graph.graph.value_info if value.name not in initializers]
        del graph.graph.value_info[:]
        graph.graph.value_info.extend(retained)
        onnx.save(graph, prepared)
        quantize_dynamic(prepared, target, weight_type=QuantType.QUInt8, per_channel=False, op_types_to_quantize=["MatMul", "Gemm"])
        onnx.checker.check_model(onnx.load(target))
        sessions = {"fp32": Compression.runtime(source), "int8": Compression.runtime(target)}
        _, validation, test = data.split(seed)
        rows = data.values[validation]
        scores = {name: Compression.scores(session, rows) for name, session in sessions.items()}
        training = json.loads((output / "report.json").read_text())
        threshold = Metrics.calibrate(data.labels[validation], scores["int8"], training["max_validation_fpr"])
        baseline = Metrics.report(data.labels[validation], scores["fp32"], metadata["recommended_threshold"])
        candidate = Metrics.report(data.labels[validation], scores["int8"], threshold)
        latency = Compression.latency(sessions, rows[:1000])
        promote = (candidate["recall"] >= baseline["recall"] - 0.001
                   and candidate["false_positive_rate"] <= training["max_validation_fpr"]
                   and latency["int8"]["median_us"] < latency["fp32"]["median_us"] * 0.95
                   and target.stat().st_size < source.stat().st_size * 0.6)
        report = {"method": "dynamic uint8 weights/activations, ONNX Runtime CPU", "promotion_uses": "validation quality, single-row CPU latency and artifact bytes; never external/test tuning",
                  "latency": latency, "bytes": {"fp32": source.stat().st_size, "int8": target.stat().st_size},
                  "validation_fp32": baseline, "validation_int8": candidate, "max_validation_score_error": float(np.max(np.abs(scores["fp32"] - scores["int8"]))),
                  "promoted": promote}
        if promote:
            shutil.copy2(source, output / "model.fp32.onnx")
            shutil.copy2(output / "metadata.json", output / "metadata.fp32.json")
            shutil.copy2(output / "report.json", output / "report.fp32.json")
            shutil.copy2(output / "parity.json", output / "parity.fp32.json")
            shutil.copy2(target, source)
            metadata.update({"precision": "uint8", "reference_sha256": metadata["artifact_sha256"], "artifact_sha256": hashlib.sha256(source.read_bytes()).hexdigest(),
                             "recommended_threshold": threshold, "quantization": report})
            fixtures = json.loads((output / "parity.json").read_text())
            for fixture in fixtures:
                fixture["score"] = float(Compression.scores(sessions["int8"], np.asarray([fixture["features"]], dtype=np.float32))[0])
            (output / "parity.json").write_text(json.dumps(fixtures, indent=2) + "\n")
            training.update(metadata)
            training["validation"] = candidate
            training["test"] = Metrics.report(data.labels[test], Compression.scores(sessions["int8"], data.values[test]), threshold)
            if hasattr(data, "origins"):
                validation_origins = np.asarray(data.origins)[validation]
                training["validation_by_source"] = {name: Metrics.report(data.labels[validation][validation_origins == name], scores["int8"][validation_origins == name], threshold) for name in np.unique(validation_origins)}
                origins = np.asarray(data.origins)[test]
                test_scores = Compression.scores(sessions["int8"], data.values[test])
                training["test_by_source"] = {name: Metrics.report(data.labels[test][origins == name], test_scores[origins == name], threshold) for name in np.unique(origins)}
            (output / "metadata.json").write_text(json.dumps(metadata, indent=2) + "\n")
            (output / "report.json").write_text(json.dumps(training, indent=2) + "\n")
        (output / "compression-report.json").write_text(json.dumps(report, indent=2) + "\n")
        return report
