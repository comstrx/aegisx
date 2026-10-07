"""Explicit research-round entry point. The shipped artifact is never modified."""
import argparse
import json
from pathlib import Path
import time

import torch

from aegisx_model.module.lifecycle.unified import Unified
from aegisx_model.module.lifecycle.unified_data import UnifiedData
from aegisx_model.module.lifecycle.unified_train import UnifiedTraining

ROOT = Path(__file__).resolve().parents[1]
parser = argparse.ArgumentParser()
parser.add_argument("command", choices=["prepare", "preflight", "train"])
parser.add_argument("--data", type=Path, default=ROOT / "runs/unified-v1/data")
parser.add_argument("--pretrained", type=Path, default=ROOT / "data/modernbert")
parser.add_argument("--output", type=Path, default=ROOT / "runs/unified-v1/candidate")
parser.add_argument("--steps", type=int, nargs=3, default=[64, 256, 64])
parser.add_argument("--resume", action="store_true")
args = parser.parse_args()
if args.command == "prepare":
    report = UnifiedData.prepare(ROOT / "runs/v09/data", args.pretrained, args.data)
    print(json.dumps({k: v for k, v in report.items() if k not in ("pretrained", "files")}, indent=2))
elif args.command == "preflight":
    import numpy as np
    import resource
    torch.set_num_threads(2)
    torch.manual_seed(1002)
    data = UnifiedData(args.data)
    model = Unified.load(args.pretrained, compact=data.manifest.get("schema_version", 1) >= 2)
    counts = model.trainable(None)
    indices = np.flatnonzero(data.partitions == 0)[:4]
    started = time.perf_counter()
    logits = model(**UnifiedTraining.tensors(data, indices))
    logits.square().mean().backward()
    report = {**counts, "forward_backward_seconds": time.perf_counter()-started,
              "max_rss_kib": resource.getrusage(resource.RUSAGE_SELF).ru_maxrss,
              "gradients_finite": all(torch.isfinite(p.grad).all().item() for p in model.parameters() if p.grad is not None),
              "no_optimizer_step": True, "logits_shape": list(logits.shape)}
    (args.data.parent / "preflight.json").write_text(json.dumps(report, indent=2)+"\n")
    print(json.dumps(report))
else:
    if any(value < 1 or value > 4096 for value in args.steps): raise ValueError("Steps must be 1..4096")
    schedule = [{"name": "joint-inputs", "upper_layers": 0, "steps": args.steps[0], "lr": .0003},
                {"name": "upper-backbone", "upper_layers": 4, "steps": args.steps[1], "lr": .00005},
                {"name": "full-backbone", "upper_layers": None, "steps": args.steps[2], "lr": .00002, "optimizer": "adafactor"}]
    UnifiedTraining.run(args.data, args.pretrained, args.output, schedule, resume=args.resume)
