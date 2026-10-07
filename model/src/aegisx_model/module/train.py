import json
import os
import random
from pathlib import Path

import numpy as np
import torch

from ..core import ModelError
from .features import Features
from .checkpoint import Checkpoint
from .data import Dataset
from .export import Export
from .fit import Fit
from .metrics import Metrics
from .network import Network
from .options import TrainingOptions


class Trainer:

    def run ( self, data: Dataset, output: Path, epochs: int, seed: int, resume=None, max_fpr=0.01, options=None ):

        options = options or TrainingOptions()
        options.validate()
        if not 1 <= epochs <= 10000: raise ModelError("Epochs must be between 1 and 10000")
        if not 0 <= seed < 2**32 or not 0 <= max_fpr <= 1: raise ModelError("Invalid seed or calibration budget")
        if options.device == "cuda" and not torch.cuda.is_available(): raise ModelError("CUDA is unavailable in this environment")
        os.environ.setdefault("CUBLAS_WORKSPACE_CONFIG", ":4096:8")
        torch.set_num_threads(1)
        torch.manual_seed(seed)
        random.seed(seed)
        np.random.seed(seed)
        torch.use_deterministic_algorithms(True)
        train, validation, test = data.split(seed)
        network = Network().to(options.device)
        optimizer = torch.optim.AdamW(network.parameters(), lr=options.learning_rate, weight_decay=options.weight_decay)
        fingerprint = data.fingerprint()
        checkpoint = Checkpoint(output, {
            "seed": seed, "fingerprint": fingerprint, "feature_version": Features().version,
            "source": data.source, "architecture": network.architecture, "max_fpr": max_fpr, "options": options.signature(),
        })
        if resume is not None: checkpoint.restore(resume, network, optimizer, epochs)
        start = checkpoint.epoch
        output.mkdir(parents=True, exist_ok=True)
        weight = Fit.run(network, optimizer, checkpoint, data, train, validation, epochs, seed, options, max_fpr)
        network.load_state_dict(checkpoint.best)
        network.cpu().eval()
        # Unsupported source features must not contribute arbitrary untrained weights.
        supported = np.any(data.values[train] != 0, axis=0)
        with torch.no_grad(): network.layers[0].weight[:, ~torch.from_numpy(supported)] = 0
        metadata = Export.write(network, output, data.source, data.values[test])
        validation_scores = Export.scores(output, data.values[validation])
        test_scores = Export.scores(output, data.values[test])
        threshold = Metrics.calibrate(data.labels[validation], validation_scores, max_fpr)
        report = {
            "provenance": getattr(data, "provenance", {}), "supported_features": supported.tolist(),
            "source": data.source, "seed": seed, "epochs": checkpoint.epoch, "target_epochs": epochs, "resumed_epoch": start,
            "best_epoch": checkpoint.best_epoch, "best_selection_value": checkpoint.best_loss, "checkpoint_selection": options.selection,
            "stopped_early": checkpoint.epoch < epochs, "options": options.signature(), "device": options.device,
            "training_positive_weight": weight, "data_fingerprint": fingerprint, "max_validation_fpr": max_fpr,
            "splits": {name: {"rows": int(mask.sum()), "groups": len(np.unique(data.groups[mask]))}
                       for name, mask in (("train", train), ("validation", validation), ("test", test))},
            "validation": Metrics.report(data.labels[validation], validation_scores, threshold),
            "test": Metrics.report(data.labels[test], test_scores, threshold),
            "recommended_threshold": threshold, "deployment_ready": False,
            "notice": "Source-specific evaluation only. Field evaluation is required before automatic enforcement; missing source features are disabled.",
        }
        if hasattr(data, "signatures"):
            (output / "source-signatures.json").write_text(json.dumps(data.signatures) + "\n")
        if hasattr(data, "origins"):
            valid_origins = np.asarray(data.origins)[validation]
            report["validation_by_source"] = {source: Metrics.report(data.labels[validation][valid_origins == source], validation_scores[valid_origins == source], threshold)
                                               for source in np.unique(valid_origins)}
            origins = np.asarray(data.origins)[test]
            report["test_by_source"] = {source: Metrics.report(data.labels[test][origins == source], test_scores[origins == source], threshold)
                                        for source in np.unique(origins)}
        report["calibration_runtime"] = "ONNX Runtime CPU; exact validation score boundaries, no held-out tuning"
        metadata.update({"supported_features":supported.tolist(), "provenance":getattr(data,"provenance",{}), "recommended_threshold": threshold, "training_fingerprint": fingerprint, "deployment_ready": False})
        report.update(metadata)
        (output / "metadata.json").write_text(json.dumps(metadata, indent=2) + "\n")
        (output / "report.json").write_text(json.dumps(report, indent=2) + "\n")

        return report
