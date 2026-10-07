"""Add executed transaction contrasts; preserve public source ownership and grouped splits."""
import json
from pathlib import Path

from aegisx_model.module.lifecycle.corpus import Corpus
from aegisx_model.module.lifecycle.transactions import Transactions

root = Path(__file__).resolve().parents[1]
output = root / "runs/v09/data"
if output.exists(): raise SystemExit("Refusing to overwrite a prepared corpus")
public, provenance = Corpus.public(root)
previous = [json.loads(line) for line in (root / "runs/v08/recorded-lab.jsonl").read_text().splitlines()]
transactions = Transactions.collect(128)
records = public + previous + transactions
provenance["executed_transactions"] = {"families": 128, "rows": len(transactions),
    "oracle": "Compare committed isolated SQLite balances with authentication, amount and funds policy",
    "limitations": "Controlled fixture; not independent production traffic or general fraud intelligence"}
report = Corpus.build(records, output, provenance)
(root / "runs/v09/recorded-transactions.jsonl").write_text("\n".join(json.dumps(item) for item in transactions)+"\n")
print(json.dumps(report), flush=True)
