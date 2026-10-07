"""Write byte/order/normalization reference fixtures consumed by Rust tests."""
import json
from pathlib import Path
from aegisx_model.module.lifecycle.inputs import Inputs

records = [
    {},
    {"request": b"POST /\n\x00\xff" + "مرحبا".encode(), "response": "x" * 1100,
     "admission": [1, 0, 12, 0, 2, 4, 1, 10, 0, 2, 4, 9, 12, 0, 2, 0],
     "outcome": [403, 75, 100, 1100, 1, 0, 1, 1],
     "events": [
        {"service": "api", "operation": "request", "state": "started", "span_id": "root"},
        {"service": "db", "operation": "write", "state": "completed", "span_id": "child",
         "parent_id": "root", "duration_ms": 86400000, "elapsed_ms": 70},
        {"service": "api", "operation": "request", "state": "completed", "span_id": "root"},
        {"service": "api", "operation": "after", "state": "failed", "parent_id": "root"},
        {"service": "db", "operation": "unknown", "state": "started", "parent_id": "missing"}]},
    {"request": "z" * 1100, "events": [{"service": "application", "operation": "read", "state": "completed"}] * 34}
]
fixtures = []
inputs = Inputs()
for record in records:
    request, response = inputs.bytes(record.get("request")), inputs.bytes(record.get("response"))
    events = [{"request_id": "00000000-0000-0000-0000-000000000001", "duration_ms": None} | event for event in record.get("events", [])]
    envelope = {"cache_scores": True, "journal": False, "admission": record.get("admission", [0] * 16),
                "sample": list(request), "sample_seen": len(request), "response_sample": list(response),
                "response_seen": len(response), "response_available": bool(response),
                "events_truncated": False, "outcome": record.get("outcome", [0] * 8),
                "request_id": "00000000-0000-0000-0000-000000000001", "backend_events": events}
    fixtures.append({"envelope": envelope, "expected": {name: value.reshape(-1).tolist() for name, value in inputs.record(record).items()}})
Path("weights/lifecycle-parity.json").write_text(json.dumps(fixtures, indent=2) + "\n")
