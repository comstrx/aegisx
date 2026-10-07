import json

import numpy as np
import pytest
import torch

from aegisx_model.module.lifecycle.corpus import Corpus
from aegisx_model.module.lifecycle.inputs import Inputs
from aegisx_model.module.lifecycle.network import LifecycleNetwork
from aegisx_model.module.lifecycle.transactions import Transactions


def test_transaction_oracle_distinguishes_rollback_and_unauthorized_commit ():
    rows = Transactions.collect(2)
    assert len(rows) == 48
    assert {row["label"] for row in rows} == {0, 1}
    for row in rows:
        body = json.loads(row["request"].split("\n", 1)[1])
        response = json.loads(row["response"])
        allowed = body["actor"] == 1 and 0 < body["amount"] <= 100
        assert row["label"] == int(not allowed and response["balances"] != [100, 100])
        if row["fixture"]["rollback"]: assert row["label"] == 0
        if row["fixture"]["early"]: assert row["label"] == 0
    by_request = {}
    for row in rows: by_request.setdefault(row["request"], set()).add(row["label"])
    assert any(labels == {0, 1} for labels in by_request.values())


def test_labels_and_group_ownership_are_part_of_corpus_integrity ( tmp_path ):
    Corpus.build([{"request": "GET /api", "label": 0, "group": "capture-1", "origin": "fixture"}], tmp_path, {})
    original = Corpus(tmp_path).manifest["fingerprint"]
    labels = np.load(tmp_path / "labels.npy")
    labels[0] = 1
    np.save(tmp_path / "labels.npy", labels)
    with pytest.raises(ValueError, match="Corpus changed"): Corpus(tmp_path)
    report = Corpus.build([{"request": "GET /api", "label": 1, "group": "capture-1", "origin": "fixture"}], tmp_path, {})
    assert original != report["fingerprint"]


def test_compact_model_uses_every_modality_and_preserves_absence ():
    torch.set_num_threads(1)
    model = LifecycleNetwork(variant="compact").eval()
    assert model.parameters_count() < 1000000
    record = Inputs().record({"request": "POST /transfer\n{\"amount\":10}", "response": "{\"ok\":false}",
                             "events": [{"service": "app", "operation": "transaction.commit", "state": "completed"}]})
    scores = model(*(torch.from_numpy(value[None]) for value in record.values()))
    (scores[1]+scores[2]).sum().backward()
    for layer in (model.text_embedding, model.event_embedding, model.event_project, model.fusion[0]):
        assert layer.weight.grad.abs().sum() > 0
    empty = Inputs().record({})
    assert all(value.item() == 0 for value in model(*(torch.from_numpy(value[None]) for value in empty.values())))


def test_normalization_rejects_nonfinite_timestamps_and_ambiguous_text ():
    for value in (float("nan"), float("inf"), -1, "50"):
        with pytest.raises(ValueError): Inputs.time(value)
    with pytest.raises(ValueError): Inputs.bytes({"token": "hidden"})
    assert Inputs.time(None) == 0
    assert Inputs.time(864000000) == 1
