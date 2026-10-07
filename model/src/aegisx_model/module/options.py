from dataclasses import asdict, dataclass
import math

from ..core import ModelError


@dataclass(frozen=True)
class TrainingOptions:

    batch_size: int = 256
    learning_rate: float = 0.01
    weight_decay: float = 0.01
    balance: bool = False
    source_balance: bool = False
    selection: str = "loss"
    clip_norm: float = 1.0
    patience: int = 0
    min_delta: float = 0.00001
    checkpoint_every: int = 10
    device: str = "cpu"

    def validate ( self ):

        if not 1 <= self.batch_size <= 1048576: raise ModelError("Invalid batch size")
        if not 0 <= self.patience <= 10000 or not 1 <= self.checkpoint_every <= 1000: raise ModelError("Invalid training interval")
        bounds = ((self.learning_rate, 1e-8, 1), (self.weight_decay, 0, 1), (self.clip_norm, 1e-6, 1000), (self.min_delta, 0, 1))
        if any(not math.isfinite(value) or not low <= value <= high for value, low, high in bounds):
            raise ModelError("Invalid optimizer or early stopping configuration")
        if self.selection not in {"loss", "recall"}: raise ModelError("Invalid checkpoint selection")
        if self.device not in {"cpu", "cuda"}: raise ModelError("Device must be cpu or cuda")

    def signature ( self ):

        result = asdict(self)
        result.pop("device")
        result.pop("checkpoint_every")
        return result
