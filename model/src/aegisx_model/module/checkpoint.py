from pathlib import Path

import torch

from ..core import ModelError


class Checkpoint:

    def __init__ ( self, output: Path, signature: dict ):

        self.output = output
        self.signature = signature
        self.epoch = 0
        self.best_epoch = 0
        self.best_loss = float("inf")
        self.best = None
        self.stale = 0

    def restore ( self, path, network, optimizer, epochs ):

        state = torch.load(path, map_location="cpu", weights_only=True)
        if state.get("format") != 3 or state.get("signature") != self.signature:
            raise ModelError("Checkpoint does not match data, optimizer, split seed or feature version")
        self.epoch = state["epoch"]
        if not 0 <= self.epoch <= epochs: raise ModelError("Target epochs precede checkpoint")
        network.load_state_dict(state["model"])
        optimizer.load_state_dict(state["optimizer"])
        self.best, self.best_loss, self.best_epoch, self.stale = (state[key] for key in ("best", "best_loss", "best_epoch", "stale"))
        torch.set_rng_state(state["torch_rng"])
        device = next(network.parameters()).device
        if device.type == "cuda" and state["cuda_rng"]: torch.cuda.set_rng_state(state["cuda_rng"][0], device)

    def observe ( self, network, loss, epoch, delta ):

        self.epoch = epoch
        if loss < self.best_loss - delta:
            self.best = {key: value.detach().cpu().clone() for key, value in network.state_dict().items()}
            self.best_loss, self.best_epoch, self.stale = loss, epoch, 0
        else:
            self.stale += 1

    def save ( self, network, optimizer ):

        temporary = self.output / "checkpoint.pt.tmp"
        torch.save({
            "format": 3, "signature": self.signature, "model": network.state_dict(), "optimizer": optimizer.state_dict(),
            "epoch": self.epoch, "best": self.best, "best_loss": self.best_loss, "best_epoch": self.best_epoch, "stale": self.stale,
            "torch_rng": torch.get_rng_state(), "cuda_rng": [torch.cuda.get_rng_state(next(network.parameters()).device)] if next(network.parameters()).is_cuda else [],
        }, temporary)
        temporary.replace(self.output / "checkpoint.pt")
