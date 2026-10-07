import numpy as np
import pytest
import torch
from transformers import ModernBertConfig, ModernBertModel

from aegisx_model.module.lifecycle.inputs import Inputs
from aegisx_model.module.lifecycle.prediction import PredictionSet
from aegisx_model.module.lifecycle.unified import UnifiedNetwork
from aegisx_model.module.lifecycle.worlds import Worlds


def test_rollback_cannot_undo_disclosure_but_can_undo_a_write ():
    read = Worlds.case("atlas", 0, False, False, True, "DROP TABLE tutorial", "read")
    write = Worlds.case("atlas", 0, False, False, True, "DROP TABLE tutorial", "write")
    safe = Worlds.case("atlas", 0, False, True, True, "DROP TABLE tutorial", "read")
    assert read["label"] == 1 and read["oracle"]["private_data_returned"]
    assert write["label"] == safe["label"] == 0
    assert read["request"] == safe["request"]
    assert read["events"] != safe["events"]


def test_oracle_and_fixture_metadata_never_change_model_inputs ():
    row = Worlds.case("orbit", 0, False, False, False, "example", "write")
    changed = row | {"label": 0, "oracle": {}, "fixture": {}, "application": "fake", "split": "train"}
    left, right = Inputs().record(row), Inputs().record(changed)
    for key in left: np.testing.assert_array_equal(left[key], right[key])


def test_single_backbone_gets_gradients_from_all_modalities ():
    torch.set_num_threads(2)
    config = ModernBertConfig(hidden_size=32, intermediate_size=64, num_hidden_layers=2,
                             num_attention_heads=4, vocab_size=128, pad_token_id=0,
                             cls_token_id=1, sep_token_id=2, local_attention=32)
    model = UnifiedNetwork(ModernBertModel(config))
    batch = {"request_ids": torch.ones(2, 4, dtype=torch.long), "request_mask": torch.ones(2, 4),
             "response_ids": torch.full((2, 4), 2, dtype=torch.long), "response_mask": torch.ones(2, 4),
             "event_ids": torch.ones(2, 32, 8, dtype=torch.long), "event_mask": torch.ones(2, 32, 8),
             "event_values": torch.rand(2, 32, 8), "features": torch.rand(2, 296), "coverage": torch.rand(2, 4)}
    batch["event_values"][:, :, 0] = 1
    loss = model(**batch).square().sum()
    loss.backward()
    for parameter in [model.numeric.weight, model.event_values.weight, model.coverage.weight,
                      model.types.weight, model.backbone.layers[-1].mlp.Wi.weight]:
        assert torch.isfinite(parameter.grad).all() and parameter.grad.abs().sum() > 0
    counts = model.trainable(0)
    assert counts["pretrained_trainable_parameters"] < counts["trainable_parameters"]
    assert model.trainable(None)["trainable_parameters"] == counts["total_parameters"]


def test_prediction_sets_abstain_without_inventing_ood_probability ():
    fitted = PredictionSet.fit([0, 0, 1, 1], [.2, .8, .2, .8], alpha=.4)
    assert PredictionSet.apply([.1, .5, .9], fitted).tolist() == [0, -1, 1]
    small = PredictionSet.fit([0, 1], [.1, .9], alpha=.01)
    assert PredictionSet.apply([.1, .9], small).tolist() == [-1, -1]
    with pytest.raises(ValueError): PredictionSet.apply([float("nan")], fitted)


def test_failed_selection_never_constructs_inference_or_consumes_test_predictions (tmp_path, monkeypatch):
    import json
    from types import SimpleNamespace
    from aegisx_model.module.lifecycle import unified_evaluate
    candidate = tmp_path / "candidate"
    candidate.mkdir()
    torch.save({"data_fingerprint": "fixture", "stage": {"name": "fixture"}, "counts": {},
                "validation": {"0": {"recall": .3}, "1": {"recall": .5}}}, candidate / "selected.pt")
    monkeypatch.setattr(unified_evaluate, "UnifiedData", lambda _: SimpleNamespace(manifest={"fingerprint": "fixture"}))
    def forbidden (*_):
        raise AssertionError("Rejected candidate must not run calibration/test inference")
    monkeypatch.setattr(unified_evaluate.Unified, "load", forbidden)
    result = unified_evaluate.UnifiedEvaluation.run(tmp_path, tmp_path, candidate, tmp_path)
    assert not result["promotion"]["allowed"]
    assert result["calibration_and_test_consumed"] is False
    assert json.loads((candidate / "assessment.json").read_text()) == result


def test_new_modality_initialization_is_small_and_checkpoint_writes_are_atomic (tmp_path, monkeypatch):
    from aegisx_model.module.lifecycle.unified_train import UnifiedTraining
    model = UnifiedNetwork(ModernBertModel(ModernBertConfig(hidden_size=32, intermediate_size=64,
        num_hidden_layers=2, num_attention_heads=4, vocab_size=128, pad_token_id=0, cls_token_id=1, sep_token_id=2)))
    assert model.types.weight.std() < .004
    assert model.numeric.weight.std() < .004
    path = tmp_path / "checkpoint.pt"
    UnifiedTraining.save({"value": torch.tensor([1.0])}, path)
    def broken (value, stream):
        stream.write(b"partial")
        raise OSError("disk failure")
    monkeypatch.setattr(torch, "save", broken)
    with pytest.raises(OSError): UnifiedTraining.save({}, path)
    assert torch.load(path, weights_only=True)["value"].item() == 1
    assert not path.with_suffix(".pt.pending").exists()


def test_tokenizer_truncation_is_visible_without_mutating_baseline_coverage ():
    from aegisx_model.module.lifecycle.unified_data import UnifiedData
    class Tokenizer:
        def __call__ (self, texts, **options):
            lengths = np.asarray([len(text) for text in texts])
            if options.get("return_length"): return {"length": lengths}
            shape = (len(texts), options["max_length"])
            return {"input_ids": np.ones(shape), "attention_mask": np.ones(shape)}
    row = Inputs().record({"request": "a"*140, "response": "b"*80,
                           "events": [{"service": "s"*20, "operation": "query"}]})
    arrays = {key: np.stack([value]) for key, value in row.items()}
    encoded = UnifiedData.encode(arrays, Tokenizer())
    assert encoded["coverage"][0].tolist() == [1, 1, 1, 1]
    assert arrays["coverage"][0].tolist() == [0, 0, 0, 1]
    assert encoded["request_ids"].shape == (1, 128)
    assert encoded["event_ids"].shape == (1, 32, 16)


def test_retry_oracle_checks_committed_effects_not_retry_presence ():
    from aegisx_model.module.lifecycle.retries import Retries
    bad = Retries.case("atlas", 0, True, False, False)
    safe = Retries.case("atlas", 0, True, True, False)
    rolled_back = Retries.case("atlas", 0, True, False, True)
    assert bad["oracle"]["committed_effects"] == 2 and bad["label"] == 1
    assert safe["oracle"]["committed_effects"] == 1 and safe["label"] == 0
    assert rolled_back["oracle"]["committed_effects"] == 0 and rolled_back["label"] == 0
    assert bad["request"] == safe["request"] == rolled_back["request"]
    assert bad["response"] == safe["response"] == rolled_back["response"]


def test_compaction_preserves_logits_when_only_padding_changes ():
    torch.manual_seed(11)
    config = ModernBertConfig(hidden_size=32, intermediate_size=64, num_hidden_layers=2,
        num_attention_heads=4, vocab_size=128, pad_token_id=0, cls_token_id=1, sep_token_id=2)
    model = UnifiedNetwork(ModernBertModel(config), compact=True).eval()
    batch = {"request_ids": torch.ones(1, 3, dtype=torch.long), "request_mask": torch.ones(1, 3),
        "response_ids": torch.full((1, 2), 2, dtype=torch.long), "response_mask": torch.ones(1, 2),
        "event_ids": torch.ones(1, 2, 4, dtype=torch.long), "event_mask": torch.ones(1, 2, 4),
        "event_values": torch.ones(1, 2, 8), "features": torch.rand(1, 296), "coverage": torch.rand(1, 4)}
    padded = dict(batch)
    padded["request_ids"] = torch.nn.functional.pad(batch["request_ids"], (0, 7))
    padded["request_mask"] = torch.nn.functional.pad(batch["request_mask"], (0, 7))
    padded["response_ids"] = torch.nn.functional.pad(batch["response_ids"], (0, 11))
    padded["response_mask"] = torch.nn.functional.pad(batch["response_mask"], (0, 11))
    with torch.inference_mode(): torch.testing.assert_close(model(**batch), model(**padded), rtol=1e-5, atol=1e-6)
