import json
from pathlib import Path

import numpy as np
import torch

from aegisx_model.module.lifecycle.corpus import Corpus
from aegisx_model.module.lifecycle.inputs import Inputs
from aegisx_model.module.lifecycle.lab import Lab
from aegisx_model.module.lifecycle.network import LifecycleNetwork


def test_bytes_missing_modalities_parent_order_and_budgets ():
    inputs = Inputs()
    item = {"request": b"\x00\xff" + "مرحبا".encode(), "response": "x" * 1100,
            "events": [{"service": "api", "operation": "request", "state": "started", "span_id": "root"},
                       {"service": "db", "operation": "read", "state": "completed", "parent_id": "root",
                        "duration_ms": 86400000, "elapsed_ms": 10}]}
    feed = inputs.record(item)
    assert feed["text"][0, :2].tolist() == [1, 256]
    assert feed["text"].shape == (2, 1024)
    assert feed["coverage"].tolist() == [0, 1, 0, 1]
    assert feed["event_values"][1, [4, 6, 7]].tolist() == [1, 1 / 32, 1]
    blank = inputs.record({})
    assert not blank["text"].any() and not blank["event_values"].any()
    other = inputs.record(item | {"label": 1, "origin": "secret", "scenario": "vulnerable"})
    for key in feed: np.testing.assert_array_equal(feed[key], other[key])


def test_connected_groups_preserve_capture_template_and_identical_input ( tmp_path ):
    records = [
        {"request": "GET /a", "label": 0, "group": "one", "owner": "capture", "origin": "test"},
        {"request": "GET /b", "label": 0, "group": "two", "owner": "capture", "origin": "test"},
        {"request": "GET /b", "label": 0, "group": "three", "origin": "test"},
        {"request": "conflict", "label": 0, "group": "four", "origin": "test"},
        {"request": "conflict", "label": 1, "group": "five", "origin": "test"}]
    report = Corpus.build(records, tmp_path, {})
    assert report["conflicting_rows_removed"] == 2
    assert len(set(Corpus(tmp_path).groups)) == 1


def test_recorded_journey_oracle_distinguishes_identical_http_exchange ():
    records = Lab.collect(256)
    safe, late = records[1], records[129]
    assert safe["request"] == late["request"] and safe["response"] == late["response"]
    assert (safe["label"], late["label"]) == (0, 1)
    safe_operations = [(event["operation"], event["state"]) for event in safe["events"]]
    late_operations = [(event["operation"], event["state"]) for event in late["events"]]
    assert ("repository.write", "completed") not in safe_operations
    assert late_operations.index(("repository.write", "completed")) < late_operations.index(("authorization", "failed"))


def test_network_has_real_trainable_paths_and_missing_inputs_are_explicit ():
    torch.set_num_threads(1)
    model = LifecycleNetwork().eval()
    assert model.parameters_count() == 10901802
    feed = Inputs().record({"request": "POST /api\n{\"message\":\"hello\"}", "response": "{\"ok\":true}",
                           "events": [{"service": "app", "operation": "write", "state": "completed"}]})
    batch = [torch.from_numpy(value[None]) for value in feed.values()]
    risk, content, journey = model(*batch)
    assert torch.equal(risk, torch.maximum(content, journey))
    (content + journey).sum().backward()
    for layer in (model.text_embedding, model.event_embedding, model.event_project, model.fusion[0]):
        assert layer.weight.grad.abs().sum() > 0
    blank = [torch.from_numpy(value[None]) for value in Inputs().record({}).values()]
    assert all(value.item() == 0 for value in model(*blank))


def test_shipped_bundle_integrity_and_declared_training_support ():
    import hashlib
    root = Path(__file__).parents[1]
    metadata = json.loads((root / "weights/metadata.json").read_text())
    assert metadata["input_schema"] == "byte-journey-v1"
    assert not metadata["deployment_ready"]
    assert hashlib.sha256((root / "weights/model.onnx").read_bytes()).hexdigest() == metadata["artifact_sha256"]
    # Content head uses only indices backed by public labeled content.
    mask = metadata["supported_features"]
    assert all(mask[index] == 1 for index in list(range(16, 32)) + list(range(40, 296)))
