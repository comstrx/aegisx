"""Three predetermined training attempts; selection never reads test/external metrics."""
import json
import shutil
import time
from pathlib import Path

import numpy as np

from aegisx_model.module.sources import Sources
from aegisx_model.module.options import TrainingOptions
from aegisx_model.module.train import Trainer
from aegisx_model.module.compress import Compression

output = Path("runs/v07")
output.mkdir(parents=True, exist_ok=True)
ledger = output / "attempts.json"
if ledger.exists():
    raise SystemExit("This round already started. Inspect the ledger; never silently exceed three attempts.")
ledger.write_text(json.dumps({"budget": 3, "attempts": []}, indent=2))
start = time.perf_counter()
data = Sources.http_params(Path("data/sources/payload_full.csv"), Path("data/fuzzdb"),
                           Path("data/wcp"), augment=True, security=Path("data/seclists"))
print("Prepared", len(data.values), "rows", data.provenance, flush=True)
experiments = [
    dict(learning_rate=.001, source_balance=False, selection="loss"),
    dict(learning_rate=.0005, source_balance=True, selection="loss"),
    dict(learning_rate=.0003, source_balance=True, selection="recall"),
]
results = []
for number, settings in enumerate(experiments, 1):
    state = json.loads(ledger.read_text())
    if len(state["attempts"]) >= 3:
        raise SystemExit("Training budget exhausted")
    state["attempts"].append({"number": number, "settings": settings, "status": "running"})
    ledger.write_text(json.dumps(state, indent=2) + "\n")
    attempt = output / f"attempt-{number}"
    clock = time.perf_counter()
    report = Trainer().run(data, attempt, epochs=80, seed=42, max_fpr=.005,
                           options=TrainingOptions(batch_size=256, balance=True, patience=12,
                                                   checkpoint_every=5, **settings))
    # A macro average avoids the largest attack source hiding smaller families.
    family = [row["recall"] for row in report["validation_by_source"].values()
              if row["true_positives"] + row["false_negatives"] > 0]
    score = float(np.mean(family))
    result = {"number": number, "settings": settings, "status": "finished",
              "seconds": time.perf_counter() - clock, "epochs": report["epochs"],
              "best_epoch": report["best_epoch"], "validation": report["validation"],
              "macro_attack_recall": score, "artifact": str(attempt)}
    results.append(result)
    state["attempts"][-1] = result
    ledger.write_text(json.dumps(state, indent=2) + "\n")
    print("ATTEMPT", json.dumps(result), flush=True)

winner = max(results, key=lambda row: (row["macro_attack_recall"],
                                      row["validation"]["recall"], -row["number"]))
artifact = Path(winner["artifact"])
compression = Compression.run(data, artifact)
destination = Path("weights")
# Retain all candidates in runs; distribute only the validation-selected model.
for path in artifact.iterdir():
    if path.is_file() and path.suffix != ".pt":
        shutil.copy2(path, destination / path.name)
shutil.copy2("data/seclists/LICENSE", destination / "SECLISTS-LICENSE.txt")
(destination / "SECLISTS-NOTICE.txt").write_text(
    "Selected SecLists textual fuzzing dictionaries; MIT.\n"
    "https://github.com/danielmiessler/SecLists\n"
    "Pinned commit and individual file attribution/digests: model/sources.json.\n"
    "Dictionary membership is context-dependent, not adjudicated security ground truth.\n")
state = json.loads(ledger.read_text())
state.update({"selected_attempt": winner["number"],
              "selection": "Macro validation attack-origin recall at validation FPR <=0.5%; ties use validation recall. Test/external metrics never used.",
              "total_seconds": time.perf_counter() - start,
              "compression_promoted": compression["promoted"]})
ledger.write_text(json.dumps(state, indent=2) + "\n")
(destination / "training-round.json").write_text(json.dumps(state, indent=2) + "\n")
print("ROUND_COMPLETE", json.dumps(state), flush=True)
