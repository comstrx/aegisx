import csv
import json
import sqlite3

import numpy as np
import pytest
import torch

from aegisx_model.core import ModelError
from aegisx_model.module.data import Dataset
from aegisx_model.module.evaluate import Evaluation
from aegisx_model.module.local import LocalData
from aegisx_model.module.metrics import Metrics
from aegisx_model.module.train import Trainer


def test_resume_is_identical_and_rejects_changed_data ( tmp_path ):

    data = Dataset.demo(42)
    full, resumed = tmp_path / "full", tmp_path / "resumed"
    Trainer().run(data, full, 6, 42)
    Trainer().run(data, resumed, 3, 42)
    report = Trainer().run(data, resumed, 6, 42, resumed / "checkpoint.pt")
    assert report["resumed_epoch"] == 3
    expected = torch.load(full / "checkpoint.pt", weights_only=True)["model"]
    actual = torch.load(resumed / "checkpoint.pt", weights_only=True)["model"]
    assert all(torch.equal(expected[key], actual[key]) for key in expected)
    assert not report["deployment_ready"]
    data.values[0, 0] = 0
    with pytest.raises(ModelError, match="does not match"):
        Trainer().run(data, resumed, 7, 42, resumed / "checkpoint.pt")
    with pytest.raises(ModelError, match="does not match"):
        Trainer().run(Dataset.demo(42), resumed, 7, 43, resumed / "checkpoint.pt")
    evaluation = Evaluation.run(Dataset.demo(42), full)
    assert evaluation["same_as_training_data"]
    assert evaluation["rows"] == len(data.values)
    with (full / "model.onnx").open("ab") as output: output.write(b"changed")
    with pytest.raises(ModelError, match="hash"):
        Evaluation.run(data, full)


def test_calibration_respects_false_positive_budget ():

    truth = np.array([0, 0, 0, 1, 1])
    scores = np.array([0.1, 0.2, 0.7, 0.6, 0.9])
    threshold = Metrics.calibrate(truth, scores, 0)
    report = Metrics.report(truth, scores, threshold)
    assert report["false_positive_rate"] == 0
    assert report["recall"] == 0.5
    assert report["precision"] == 1


def test_local_export_requires_independent_labels ( tmp_path ):

    database, labels, output = tmp_path / "events.db", tmp_path / "labels.csv", tmp_path / "data.jsonl"
    values = [0.25] * 296
    with sqlite3.connect(database) as connection:
        connection.execute("CREATE TABLE events(request_id TEXT, stage TEXT, sequence INTEGER, payload TEXT)")
        connection.execute("INSERT INTO events VALUES(?, 'analyzed', 2, ?)", ("request-1", json.dumps({
            "details": {"features": values, "feature_version": 4, "risk_score": 0.999},
        })))
    with labels.open("w", newline="") as stream:
        writer = csv.DictWriter(stream, fieldnames=["request_id", "label", "group"])
        writer.writeheader()
        writer.writerow({"request_id": "request-1", "label": 0, "group": "independent-session"})
    assert LocalData.export(database, labels, output)["rows"] == 1
    data = Dataset.read(output)
    assert np.allclose(data.values[0], values)
    assert data.labels[0, 0] == 0
    labels.write_text("request_id,label,group\nrequest-1,0,session\nrequest-1,1,session\n")
    with pytest.raises(ModelError, match="duplicate"): LocalData.export(database, labels, output)
