import hashlib
import json

import numpy as np
import onnxruntime as ort

from ..core import ModelError
from .features import Features
from .metrics import Metrics


class Evaluation:

    @staticmethod
    def run ( data, artifact, threshold=None ):

        metadata = json.loads((artifact / "metadata.json").read_text())
        model = (artifact / "model.onnx").read_bytes()
        if metadata.get("feature_version") != Features().version or hashlib.sha256(model).hexdigest() != metadata.get("artifact_sha256"):
            raise ModelError("Artifact hash or feature version does not match")
        threshold = metadata.get("recommended_threshold", 0.5) if threshold is None else threshold
        if not np.isfinite(threshold) or not 0 <= threshold <= 1: raise ModelError("Threshold must be in [0, 1]")
        options = ort.SessionOptions()
        options.intra_op_num_threads = 1
        options.inter_op_num_threads = 1
        runtime = ort.InferenceSession(model, sess_options=options, providers=["CPUExecutionProvider"])
        scores = np.array([runtime.run(["risk"], {"features": row.reshape(1, len(Features().names))})[0].item() for row in data.values])
        report = Metrics.report(data.labels, scores, threshold)
        report.update({
            "artifact_sha256": metadata["artifact_sha256"], "data_fingerprint": data.fingerprint(),
            "same_as_training_data": data.fingerprint() == metadata.get("training_fingerprint"),
            "notice": "Use independent evaluation data. This command never calibrates or changes model weights.",
        })

        return report
