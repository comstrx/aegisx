import argparse
import json
from pathlib import Path

from ..core import ModelError


class Cli:

    @staticmethod
    def run () -> int:

        parser = argparse.ArgumentParser(description="Train, resume, evaluate and export the AegisX behavioral model")
        parser.add_argument("command", choices=["demo", "train", "evaluate", "export-data", "train-http"])
        parser.add_argument("--data", type=Path, help="Labeled JSONL for training or independent evaluation")
        parser.add_argument("--security", type=Path, help="Verified SecLists textual attack dictionaries")
        parser.add_argument("--source-balance", action="store_true", help="Bound source/class reweighting from training rows only")
        parser.add_argument("--selection", choices=["loss", "recall"], default="loss")
        parser.add_argument("--benign", type=Path, help="Verified WCP benign captures; external captures are excluded")
        parser.add_argument("--augment-http", action="store_true", help="Group-preserving URI/JSON envelopes for both classes")
        parser.add_argument("--extra", type=Path, help="Verified supplemental HTTP corpus directory")
        parser.add_argument("--output", type=Path, default=Path("weights"))
        parser.add_argument("--epochs", type=int, default=100, help="Total target epochs, including resumed epochs")
        parser.add_argument("--seed", type=int, default=42)
        parser.add_argument("--resume", type=Path)
        parser.add_argument("--max-fpr", type=float, default=0.01, help="Validation calibration budget")
        parser.add_argument("--artifact", type=Path, default=Path("weights"))
        parser.add_argument("--threshold", type=float, help="Explicit evaluation threshold")
        parser.add_argument("--database", type=Path)
        parser.add_argument("--labels", type=Path, help="Independent CSV with request_id,label,group")
        parser.add_argument("--batch-size", type=int, default=256)
        parser.add_argument("--learning-rate", type=float, default=0.01)
        parser.add_argument("--weight-decay", type=float, default=0.01)
        parser.add_argument("--balance", action="store_true", help="Compute positive-class weight from the training split only")
        parser.add_argument("--clip-norm", type=float, default=1.0)
        parser.add_argument("--patience", type=int, default=0, help="Validation epochs without improvement; 0 disables early stopping")
        parser.add_argument("--min-delta", type=float, default=0.00001)
        parser.add_argument("--checkpoint-every", type=int, default=10)
        parser.add_argument("--device", choices=["cpu", "cuda"], default="cpu")
        options = parser.parse_args()

        try:
            if options.command == "export-data":
                from ..module.local import LocalData
                if options.database is None or options.labels is None:
                    raise ModelError("export-data requires --database and --labels; --output is a JSONL file")
                result = LocalData.export(options.database, options.labels, options.output)
            else:
                from ..module.data import Dataset
                if options.command != "demo" and options.data is None:
                    raise ModelError("This command requires --data; use demo explicitly for synthetic input")
                if not 0 <= options.seed < 2**32: raise ModelError("Seed must be in [0, 2^32)")
                if options.command == "train-http":
                    from ..module.sources import Sources
                    data = Sources.http_params(options.data, options.extra, options.benign, options.augment_http, options.security)
                else:
                    data = Dataset.demo(options.seed) if options.command == "demo" else Dataset.read(options.data)
                if options.command == "evaluate":
                    from ..module.evaluate import Evaluation
                    result = Evaluation.run(data, options.artifact, options.threshold)
                else:
                    from ..module.train import Trainer
                    from ..module.options import TrainingOptions
                    training = TrainingOptions(**{name: getattr(options, name) for name in TrainingOptions.__dataclass_fields__})
                    result = Trainer().run(data, options.output, options.epochs, options.seed, options.resume, options.max_fpr, training)
            print(json.dumps(result, indent=2))
            return 0
        except (ModelError, OSError, ValueError) as error:
            parser.exit(1, f"aegisx-model: {error}\n")


def main ():

    raise SystemExit(Cli.run())
