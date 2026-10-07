"""Export and evaluate a saved selected candidate; this does not train it."""
import argparse
from pathlib import Path
from aegisx_model.module.lifecycle.export import Export

parser = argparse.ArgumentParser()
parser.add_argument("--data", type=Path, default=Path("runs/v08/data"))
parser.add_argument("--output", type=Path, default=Path("runs/v08/attempt-1"))
args = parser.parse_args()
Export.write(args.data, args.output)
