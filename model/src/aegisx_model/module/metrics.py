import numpy as np

from ..core import ModelError


class Metrics:

    @staticmethod
    def report ( truth, scores, threshold ):

        truth = np.asarray(truth).reshape(-1).astype(bool)
        scores = np.asarray(scores).reshape(-1)
        if not len(truth) or len(truth) != len(scores) or not np.isfinite(scores).all():
            raise ModelError("Evaluation needs equal, nonempty, finite arrays")
        predicted = scores >= threshold
        tp = int((predicted & truth).sum())
        fp = int((predicted & ~truth).sum())
        fn = int((~predicted & truth).sum())
        tn = int((~predicted & ~truth).sum())

        return {
            "threshold": float(threshold), "rows": len(truth),
            "true_positives": tp, "false_positives": fp, "false_negatives": fn, "true_negatives": tn,
            "accuracy": float((predicted == truth).mean()),
            "precision": tp / max(1, tp + fp), "recall": tp / max(1, tp + fn),
            "false_positive_rate": fp / max(1, fp + tn),
            "majority_baseline_accuracy": float(max(truth.mean(), 1 - truth.mean())),
        }

    @staticmethod
    def calibrate ( truth, scores, max_fpr ):

        truth = np.asarray(truth).reshape(-1).astype(bool)
        scores = np.asarray(scores).reshape(-1)
        if not len(scores) or len(truth) != len(scores) or not np.isfinite(scores).all() or not 0 <= max_fpr <= 1:
            raise ModelError("Invalid calibration arrays or FPR budget")
        if len(np.unique(truth)) != 2: raise ModelError("Calibration requires both classes")
        order = np.argsort(-scores, kind="stable")
        ordered, labels = scores[order], truth[order]
        ends = np.r_[np.flatnonzero(ordered[:-1] != ordered[1:]), len(ordered)-1]
        tp, fp = np.cumsum(labels)[ends], np.cumsum(~labels)[ends]
        negatives = int((~truth).sum())
        allowed = np.flatnonzero(fp / negatives <= max_fpr)
        if not len(allowed):
            if scores.max() < 1: return 1.0
            raise ModelError("No supported threshold satisfies the validation FPR budget")
        # Evaluate every score boundary, including ties, in O(n log n), not coarse quantiles.
        best = max(allowed, key=lambda index: (int(tp[index]), tp[index]/max(1,tp[index]+fp[index]), ordered[ends[index]]))
        return float(ordered[ends[best]])
