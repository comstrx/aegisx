import hashlib
import json
from pathlib import Path

import numpy as np
import onnx
import onnxruntime as ort
import torch
from torch import nn

from ..core import ModelError
from .features import Features


class Export:

    @staticmethod
    def write ( network, output: Path, source: str, values ):

        output.mkdir(parents=True, exist_ok=True)
        (output / "features.json").write_text(json.dumps(Features().spec, indent=2) + "\n")
        path = output / "model.onnx"
        inference = nn.Sequential(network, nn.Sigmoid()).eval()
        sample = torch.zeros((1, len(Features().names)), dtype=torch.float32)

        torch.onnx.export(
            inference, (sample,), str(path),
            input_names=["features"], output_names=["risk"],
            opset_version=18, dynamo=True, external_data=False,
        )
        graph = onnx.load(path)
        onnx.checker.check_model(graph)
        options = ort.SessionOptions()
        options.intra_op_num_threads = 1
        options.inter_op_num_threads = 1
        runtime = ort.InferenceSession(str(path), sess_options=options, providers=["CPUExecutionProvider"])
        fixtures = []
        max_error = 0.0

        for row in np.concatenate([np.zeros((1, len(Features().names))), np.ones((1, len(Features().names))), values[:30]]).astype(np.float32):
            batch = row.reshape(1, len(Features().names))
            with torch.no_grad(): expected = float(inference(torch.from_numpy(batch)).item())
            actual = float(runtime.run(["risk"], {"features": batch})[0].item())
            if not np.isfinite([expected, actual]).all(): raise ModelError("Non-finite inference output")
            max_error = max(max_error, abs(expected - actual))
            fixtures.append({"features": row.tolist(), "score": expected})

        if max_error > 1e-5: raise ModelError(f"ONNX parity failed: {max_error}")
        metadata = {
            "model_version": "bootstrap-v6" if source == "synthetic_demo" else "lifecycle-v6",
            "artifact_sha256": hashlib.sha256(path.read_bytes()).hexdigest(),
            "feature_version": Features().version,
            "architecture": network.architecture, "parameter_count": network.parameters_count(), "source": source,
            "purpose": "HTTP content risk research; runtime effectiveness is not established",
            "max_onnx_error": max_error,
        }
        (output / "metadata.json").write_text(json.dumps(metadata, indent=2) + "\n")
        (output / "parity.json").write_text(json.dumps(fixtures, indent=2) + "\n")

        return metadata


    @staticmethod
    def scores ( output, values ):
        options = ort.SessionOptions()
        options.intra_op_num_threads = 1
        options.inter_op_num_threads = 1
        runtime = ort.InferenceSession(str(output / "model.onnx"), sess_options=options, providers=["CPUExecutionProvider"])
        return np.asarray([runtime.run(["risk"], {"features": row.reshape(1, len(Features().names))})[0].item() for row in values], dtype=np.float32)
