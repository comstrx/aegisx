import argparse
import json
from pathlib import Path

from aegisx_model.module.lifecycle.unified_evaluate import UnifiedEvaluation

ROOT = Path(__file__).resolve().parents[1]
parser = argparse.ArgumentParser()
parser.add_argument("--round", type=Path, default=ROOT / "runs/unified-v1")
parser.add_argument("--pretrained", type=Path, default=ROOT / "data/modernbert")
args = parser.parse_args()
report = UnifiedEvaluation.run(args.round / "data", args.pretrained,
                               args.round / "candidate", ROOT / "weights")
print(json.dumps(report["promotion"], indent=2))
