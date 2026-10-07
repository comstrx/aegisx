import json
from importlib.resources import files

import numpy as np

from ..core import ModelError


class Features:

    def __init__ ( self, spec=None ):

        self.spec = spec or json.loads(files("aegisx_model").joinpath("features.json").read_text())
        self.names = tuple(item["name"] for item in self.spec["features"])
        self.scales = np.array([item["scale"] for item in self.spec["features"]], dtype=np.float32)
        self.version = self.spec["version"]

    def validate ( self, rows, normalized=False ) -> np.ndarray:

        try:
            values = np.asarray(rows, dtype=np.float32)
        except (ValueError, TypeError, OverflowError) as error:
            raise ModelError("Features must be numeric") from error
        if values.ndim != 2 or values.shape[1] != len(self.names):
            raise ModelError(f"Expected a matrix with {len(self.names)} features")
        if not np.isfinite(values).all() or (values < 0).any():
            raise ModelError("Features must be finite, nonnegative numbers")

        if normalized and (values > 1).any():
            raise ModelError("Normalized features must be in [0, 1]")
        return values

    def normalize ( self, rows ) -> np.ndarray:

        return np.clip(self.validate(rows) / self.scales, 0.0, 1.0)

