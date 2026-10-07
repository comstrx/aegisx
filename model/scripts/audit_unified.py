"""Verify saved optimization evidence without consuming calibration or test examples."""
import argparse
import hashlib
import json
from pathlib import Path

from safetensors import safe_open
import torch

ROOT = Path(__file__).resolve().parents[1]
parser = argparse.ArgumentParser()
parser.add_argument("--round", type=Path, default=ROOT / "runs/unified-v1")
parser.add_argument("--pretrained", type=Path, default=ROOT / "data/modernbert")
args = parser.parse_args()
torch.set_num_threads(2)
checkpoint = torch.load(args.round / "candidate/checkpoint.pt", map_location="cpu", weights_only=True, mmap=True)
last = json.loads((args.pretrained / "config.json").read_text())["num_hidden_layers"] - 1
report = {"step": checkpoint["steps"], "stage": checkpoint["stage"],
          "optimizer_state_entries": len(checkpoint["optimizer"]["state"]),
          "fingerprint": checkpoint["data_fingerprint"], "parameter_changes": {}}
with safe_open(args.pretrained / "model.safetensors", framework="pt", device="cpu") as source:
    for name in ("layers.0.attn.Wqkv.weight", f"layers.{last}.mlp.Wi.weight"):
        original = source.get_tensor("model."+name)
        current = checkpoint["model"]["backbone."+name]
        report["parameter_changes"][name] = {"max_absolute_change": float((current-original).abs().max()),
                                             "changed_values": int((current != original).sum()), "values": current.numel()}
        assert report["parameter_changes"][name]["changed_values"] > 0
for name in ("selected.pt", "checkpoint.pt"):
    path = args.round / "candidate" / name
    with path.open("rb") as stream: digest = hashlib.file_digest(stream, "sha256").hexdigest()
    report[name] = {"bytes": path.stat().st_size, "sha256": digest}
(args.round / "audit.json").write_text(json.dumps(report, indent=2)+"\n")
print(json.dumps(report, indent=2))
