"""Measure the local frozen encoder before committing any training attempt."""
import copy
import json
import os
import time
from pathlib import Path

os.environ["HF_HUB_OFFLINE"] = "1"
os.environ["HF_HUB_DISABLE_TELEMETRY"] = "1"
import torch
from transformers import AutoModel, AutoTokenizer

torch.set_num_threads(2)
root = Path(__file__).resolve().parents[1]
path = root / "data/codebert"
tokenizer = AutoTokenizer.from_pretrained(path, local_files_only=True)
model = AutoModel.from_pretrained(path, local_files_only=True, use_safetensors=False).eval()
print("PARAMETERS", sum(p.numel() for p in model.parameters()), flush=True)
sample = tokenizer(["POST /api/search\n" + '{"query":"ordinary text","page":2}' * 12] * 8,
                   truncation=True, max_length=128, padding=True, return_tensors="pt")
report = {}
for precision, network in (("fp32", model), ("int8", torch.ao.quantization.quantize_dynamic(copy.deepcopy(model), {torch.nn.Linear}, dtype=torch.qint8))):
    times = []
    with torch.inference_mode():
        for step in range(6):
            started = time.perf_counter()
            output = network(**sample).last_hidden_state
            if step: times.append((time.perf_counter()-started)/8)
    report[precision] = {"row_ms": sorted(times)[2]*1000, "finite": bool(output.isfinite().all())}
    print(precision, report[precision], flush=True)
(root / "runs/v09/teacher-preflight.json").write_text(json.dumps(report, indent=2)+"\n")
