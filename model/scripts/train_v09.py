"""Each invocation consumes one explicitly authorized training candidate."""
import argparse
import json
import time
from pathlib import Path

from aegisx_model.module.lifecycle.round import Round
from aegisx_model.module.lifecycle.teacher import Teacher

parser = argparse.ArgumentParser()
parser.add_argument("--attempt", type=int, choices=(1, 2, 3), required=True)
args = parser.parse_args()
root = Path(__file__).resolve().parents[1]
directory = root / "runs/v09"
ledger_path = directory / "attempts.json"
ledger = json.loads(ledger_path.read_text())
if len(ledger["attempts"]) >= ledger["authorized_candidates"]: raise SystemExit("Authorized training budget exhausted")
if any(item["attempt"] == args.attempt for item in ledger["attempts"]): raise SystemExit("Attempt already consumed")
output = directory / f"attempt-{args.attempt}"
if output.exists(): raise SystemExit("Refusing to overwrite a candidate")
if args.attempt == 1 and not (directory / "teacher-cache/embedding-manifest.json").exists():
    raise SystemExit("Teacher cache not complete; no attempt consumed")
if args.attempt == 3 and not (directory / "attempt-1/teacher-logits.npy").exists():
    raise SystemExit("Teacher not trained; no attempt consumed")
output.mkdir()
record = {"attempt": args.attempt, "status": "started", "started_unix": time.time(),
          "kind": {1: "125M frozen pretrained encoder plus learned transfer head",
                   2: "supervised compact lifecycle student",
                   3: "same student with training-only teacher distillation"}[args.attempt]}
ledger["attempts"].append(record)
ledger_path.write_text(json.dumps(ledger, indent=2)+"\n")
try:
    result = Teacher.run(directory / "data", directory / "teacher-cache", output) if args.attempt == 1 else Round.run(
        directory / "data", output, directory / "attempt-1" if args.attempt == 3 else None)
    record.update(status="completed", seconds=time.time()-record["started_unix"],
                  parameters=result.get("total_parameters", result.get("parameter_count")))
except BaseException as error:
    record.update(status="failed", error=repr(error))
    raise
finally:
    # Scripts are executed sequentially; do not run competing training candidates.
    ledger_path.write_text(json.dumps(ledger, indent=2)+"\n")
