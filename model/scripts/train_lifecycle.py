"""Train one explicit text/journey candidate without overwriting saved work."""
import argparse
import json
from pathlib import Path

from aegisx_model.module.lifecycle.train import Training

parser = argparse.ArgumentParser()
parser.add_argument("--data", type=Path, default=Path("runs/v08/data"))
parser.add_argument("--output", type=Path, default=Path("runs/v08/attempt-1"))
parser.add_argument("--epochs", type=int, default=12)
args = parser.parse_args()
if not 1 <= args.epochs <= 200: parser.error("epochs must be in 1..200")
if args.output.exists() and any(args.output.iterdir()):
    raise SystemExit("This experiment already contains work. Choose a fresh output directory.")
report = Training.run(args.data, args.output, args.epochs)
print("TRAINING_COMPLETE", json.dumps({key: report[key] for key in ("parameter_count", "epochs", "best_epoch", "seconds", "tasks")}), flush=True)
