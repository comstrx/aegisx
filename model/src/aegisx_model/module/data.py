import hashlib
import json

import numpy as np

from ..core import ModelError
from .features import Features


class Dataset:

    def __init__ ( self, rows, labels, groups, source: str, normalized=False ):

        self.values = Features().validate(rows, normalized=True) if normalized else Features().normalize(rows)
        self.labels = np.asarray(labels, dtype=np.float32).reshape(-1, 1)
        self.groups = np.asarray(groups)
        self.source = source
        if len(self.values) != len(self.labels) or len(self.labels) != len(self.groups):
            raise ModelError("Features, labels and groups must have equal length")
        if not np.isin(self.labels, [0, 1]).all():
            raise ModelError("Labels must be 0 or 1")

    @classmethod
    def read ( cls, path ):

        schema = Features()
        rows, labels, groups = [], [], []
        with path.open() as stream:
            for number, line in enumerate(stream, 1):
                try:
                    item = json.loads(line)
                    if set(item["features"]) != set(schema.names):
                        raise ValueError("Feature names do not match current schema")
                    rows.append([item["features"][name] for name in schema.names])
                    labels.append(item["label"])
                    group = item["group"]
                    if not isinstance(group, str) or not group: raise ValueError("A nonempty session group is required")
                    groups.append(group)
                except (ValueError, KeyError, TypeError) as error:
                    raise ModelError(f"Invalid dataset row {number}: {error}") from error

        return cls(rows, labels, groups, "user_labeled")

    @classmethod
    def demo ( cls, seed: int ):

        rng = np.random.default_rng(seed)
        width = len(Features().names)
        rows, labels, groups = [], [], []
        for session in range(48):
            burst = session % 2 == 1
            for step in range(24):
                gap = rng.uniform(0.001, 0.1) if burst else rng.uniform(0.5, 10)
                prior = min(step, 10 / gap)
                row = [
                    1, 0, rng.integers(1, 80), 0, rng.integers(3, 15), 0, 0,
                    prior, 0, 0, min(prior, 8) if burst else 0,
                    60 if step == 0 else gap, step * gap,
                    0 if step == 0 else rng.uniform(5, 80), rng.integers(1, 5), 0,
                ]
                rows.append(row + [0]*22 + [1, 0] + [0]*(width-40))
                labels.append(int(burst and step >= 8))
                groups.append(f"synthetic-session-{session:03}")

        return cls(rows, labels, groups, "synthetic_demo")

    def fingerprint ( self ):

        digest = hashlib.sha256()
        digest.update(self.values.astype("<f4").tobytes())
        digest.update(self.labels.astype("<f4").tobytes())
        digest.update(json.dumps(self.groups.tolist(), ensure_ascii=True).encode())
        digest.update(self.source.encode())
        return digest.hexdigest()

    def split ( self, seed: int ):

        groups = np.unique(self.groups)
        if len(groups) < 12: raise ModelError("At least twelve independent session groups are required")
        rng = np.random.default_rng(seed)
        rng.shuffle(groups)
        size = max(2, len(groups) // 5)
        test = np.isin(self.groups, groups[:size])
        validation = np.isin(self.groups, groups[size:2 * size])
        train = ~(test | validation)
        for mask in (train, validation, test):
            if len(np.unique(self.labels[mask])) != 2:
                raise ModelError("Train, validation and test groups must each contain both labels")

        return train, validation, test
