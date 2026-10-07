import numpy as np
import pytest
import torch

from aegisx_model.core import ModelError
from aegisx_model.module.data import Dataset
from aegisx_model.module.options import TrainingOptions
from aegisx_model.module.train import Trainer


def test_balanced_minibatches_resume_and_optimizer_identity ( tmp_path ):

    data = Dataset.demo(42)
    options = TrainingOptions(batch_size=73, balance=True, learning_rate=0.003, checkpoint_every=1)
    full, resumed = tmp_path / "full", tmp_path / "resumed"
    Trainer().run(data, full, 5, 42, options=options)
    Trainer().run(data, resumed, 2, 42, options=options)
    report = Trainer().run(data, resumed, 5, 42, resumed / "checkpoint.pt", options=options)
    expected = torch.load(full / "checkpoint.pt", weights_only=True)
    actual = torch.load(resumed / "checkpoint.pt", weights_only=True)
    for field in ("model", "best"):
        assert all(torch.equal(expected[field][key], actual[field][key]) for key in expected[field])
    assert expected["best_epoch"] == actual["best_epoch"]
    train, _, _ = data.split(42)
    labels = data.labels[train]
    assert report["training_positive_weight"] == pytest.approx(float((labels == 0).sum() / (labels == 1).sum()))
    with pytest.raises(ModelError, match="does not match"):
        Trainer().run(data, resumed, 6, 42, resumed / "checkpoint.pt", options=TrainingOptions(batch_size=74))
    assert report["max_onnx_error"] < 1e-5


def test_early_stop_preserves_best_and_completed_optimizer ( tmp_path ):

    options = TrainingOptions(learning_rate=1e-8, min_delta=1, patience=2, batch_size=100)
    report = Trainer().run(Dataset.demo(42), tmp_path, 10, 42, options=options)
    state = torch.load(tmp_path / "checkpoint.pt", weights_only=True)
    assert report["stopped_early"]
    assert report["epochs"] == 3
    assert report["best_epoch"] == 1
    assert state["epoch"] == 3 and state["best_epoch"] == 1
    assert any(not torch.equal(state["model"][key], state["best"][key]) for key in state["model"])
    resumed = Trainer().run(Dataset.demo(42), tmp_path, 20, 42, tmp_path / "checkpoint.pt", options=options)
    assert resumed["epochs"] == 3
    assert resumed["resumed_epoch"] == 3


def test_invalid_training_options_fail_before_output ( tmp_path ):

    for options in (TrainingOptions(batch_size=0), TrainingOptions(learning_rate=np.nan),
                    TrainingOptions(patience=-1), TrainingOptions(clip_norm=0), TrainingOptions(device="invalid")):
        with pytest.raises(ModelError):
            Trainer().run(Dataset.demo(42), tmp_path / "invalid", 2, 42, options=options)
    assert not (tmp_path / "invalid").exists()
