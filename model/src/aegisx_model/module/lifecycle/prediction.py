"""Class-conditional prediction sets on a separate calibration partition."""
import math

import numpy as np


class PredictionSet:
    @staticmethod
    def fit ( labels, scores, alpha=.01 ):
        labels, scores = np.asarray(labels).reshape(-1), np.asarray(scores).reshape(-1)
        if len(labels) != len(scores) or not np.isin(labels, [0, 1]).all():
            raise ValueError("Calibration requires aligned binary labels")
        if not 0 < alpha < 1 or not np.isfinite(scores).all() or ((scores < 0) | (scores > 1)).any():
            raise ValueError("Invalid calibration inputs")
        result = {"alpha": alpha, "thresholds": [], "counts": []}
        for label in (0, 1):
            values = np.sort((scores if label == 0 else 1-scores)[labels == label])
            if not len(values): raise ValueError("Calibration requires both labels")
            rank = math.ceil((len(values)+1)*(1-alpha))
            result["thresholds"].append(float(values[rank-1]) if rank <= len(values) else 1.0)
            result["counts"].append(len(values))
        return result

    @staticmethod
    def apply ( scores, calibration ):
        scores = np.asarray(scores)
        if not np.isfinite(scores).all() or ((scores < 0) | (scores > 1)).any(): raise ValueError("Invalid probabilities")
        safe = scores <= calibration["thresholds"][0]
        attack = 1-scores <= calibration["thresholds"][1]
        # -1 is unresolved: both classes or neither class. It is not a calibrated OOD probability.
        return np.where(safe ^ attack, attack.astype(np.int64), -1)
