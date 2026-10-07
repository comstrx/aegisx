"""Progressive joint fine-tuning with atomic checkpoints and held-out selection."""
import gc
import json
import math
import os
import time

import numpy as np
import torch
from torch.nn import functional as F
from transformers.optimization import Adafactor

from ..metrics import Metrics
from .unified import Unified
from .unified_data import UnifiedData


class UnifiedTraining:
    @staticmethod
    def save ( value, path ):
        temporary = path.with_suffix(path.suffix + ".pending")
        try:
            with temporary.open("wb") as stream:
                torch.save(value, stream)
                stream.flush()
                os.fsync(stream.fileno())
            temporary.replace(path)
        finally:
            temporary.unlink(missing_ok=True)

    @staticmethod
    def tensors ( data, indices ):
        return {name: torch.from_numpy(value) for name, value in data.batch(indices).items()}

    @staticmethod
    def scores ( model, data, indices, batch_size=4 ):
        model.eval()
        values = []
        with torch.inference_mode():
            for offset in range(0, len(indices), batch_size):
                logits = model(**UnifiedTraining.tensors(data, indices[offset:offset+batch_size]))
                if not torch.isfinite(logits).all(): raise ValueError("Non-finite inference")
                values.extend(torch.sigmoid(logits).numpy().tolist())
        return np.asarray(values)

    @staticmethod
    def validation ( model, data ):
        results, objective = {}, 0.0
        for task in (0, 1):
            indices = np.flatnonzero((data.partitions == 1) & (data.tasks == task))
            indices = UnifiedData.stratified(data.labels, indices, 64, 2200+task)
            scores = UnifiedTraining.scores(model, data, indices)[:, task]
            labels = data.labels[indices]
            threshold = Metrics.calibrate(labels, scores, .01)
            report = Metrics.report(labels, scores, threshold)
            clipped = np.clip(scores, 1e-6, 1-1e-6)
            report["balanced_loss"] = float(-.5*(np.log(clipped[labels == 1]).mean()+np.log1p(-clipped[labels == 0]).mean()))
            results[str(task)] = report
            objective += report["recall"] - .05*report["balanced_loss"]
        return results, objective

    @staticmethod
    def optimizer ( model, stage ):
        parameters = [p for p in model.parameters() if p.requires_grad]
        if stage.get("optimizer") == "adafactor":
            return Adafactor(parameters, lr=stage["lr"], scale_parameter=False, relative_step=False,
                             warmup_init=False, clip_threshold=1.0, weight_decay=.01)
        return torch.optim.AdamW(parameters, lr=stage["lr"], weight_decay=.01, foreach=False)

    @staticmethod
    def learning_rate ( stage, step ):
        warmup = max(1, min(16, stage["steps"] // 8))
        if step < warmup: return stage["lr"] * (step+1) / warmup
        progress = (step-warmup) / max(1, stage["steps"]-warmup)
        return stage["lr"] * (.2 + .8 * .5 * (1 + math.cos(math.pi*progress)))

    @staticmethod
    def run ( directory, pretrained, output, schedule, batch_size=4, resume=False ):
        if batch_size != 4: raise ValueError("Joint task/counterfactual recipe requires four samples")
        authorization_path = output / "authorization.json"
        authorization = {"user_authorized": "2026-10-02 request: improve server and train a model above 200 million parameters",
                         "schedule": schedule, "batch_size": batch_size, "test_access_during_training": False,
                         "recipe_version": 2, "pretrained": json.loads((pretrained / "manifest.json").read_text()),
                         "previous_rounds": "Immutable; this new request authorizes this round only"}
        if resume:
            if json.loads(authorization_path.read_text()) != authorization: raise ValueError("Resume recipe/source mismatch")
            if (output / "training-report.json").exists(): raise ValueError("Round already complete")
        else:
            if authorization_path.exists(): raise ValueError("This candidate already started; select a fresh round")
            output.mkdir(parents=True, exist_ok=True)
            authorization_path.write_text(json.dumps(authorization, indent=2)+"\n")
        torch.set_num_threads(2)
        torch.manual_seed(1002)
        data = UnifiedData(directory)
        model = Unified.load(pretrained, compact=data.manifest.get("schema_version", 1) >= 2)
        supported = np.any(data.arrays["features"][data.partitions == 0] != 0, axis=0)
        model.supported_features.copy_(torch.from_numpy(supported.astype(np.float32)))
        rng = np.random.default_rng(1002)
        pools = [np.flatnonzero((data.partitions == 0) & (data.tasks == task) & (data.labels == label))
                 for task, label in ((0, 0), (0, 1), (1, 0), (1, 1))]
        if any(not len(pool) for pool in pools): raise ValueError("Missing training task/class")
        pairs = []
        for name in sorted(set(data.pairs[data.partitions == 0]) - {""}):
            ids = np.flatnonzero((data.partitions == 0) & (data.pairs == name))
            safe, bad = ids[data.labels[ids] == 0], ids[data.labels[ids] == 1]
            if len(safe) and len(bad): pairs.append((int(safe[0]), int(bad[0])))
        started, history, best_objective, total_steps, elapsed = time.perf_counter(), [], -float("inf"), 0, 0
        start_stage, start_step, state = 0, 0, None
        seen = set()
        if resume:
            state = torch.load(output / "checkpoint.pt", map_location="cpu", weights_only=True, mmap=True)
            if state["data_fingerprint"] != data.manifest["fingerprint"]: raise ValueError("Resume data mismatch")
            model.load_state_dict(state["model"])
            torch.set_rng_state(state["rng"])
            rng.bit_generator.state = state["numpy_rng"]
            history, total_steps, elapsed = state["history"], state["steps"], state["seconds"]
            start_stage, start_step, best_objective = state["stage_index"], state["stage_step"], state["best_objective"]
            seen = set(state["seen"])
        for stage_index, stage in enumerate(schedule):
            if stage_index < start_stage: continue
            counts = model.trainable(stage["upper_layers"])
            optimizer = UnifiedTraining.optimizer(model, stage)
            first = start_step if stage_index == start_stage else 0
            if state is not None:
                optimizer.load_state_dict(state["optimizer"])
                del state
                state = None
            for step in range(first, stage["steps"]):
                model.train()
                indices = np.asarray([rng.choice(pool) for pool in pools])
                paired = step % 2 == 0 and bool(pairs)
                if paired: indices[2:4] = pairs[rng.integers(len(pairs))]
                seen.update(map(int, indices))
                labels = torch.from_numpy(data.labels[indices].astype(np.float32))
                task = torch.from_numpy(data.tasks[indices].astype(np.int64))
                optimizer.zero_grad(set_to_none=True)
                logits = model(**UnifiedTraining.tensors(data, indices))
                selected = logits[torch.arange(batch_size), task]
                loss = F.binary_cross_entropy_with_logits(selected, labels*.98+.01)
                if paired: loss = loss + .1*F.softplus(.5-(selected[3]-selected[2]))
                if not torch.isfinite(loss): raise ValueError("Non-finite training loss")
                loss.backward()
                if stage.get("optimizer") != "adafactor":
                    torch.nn.utils.clip_grad_norm_(model.parameters(), 1.0, error_if_nonfinite=True)
                elif any(p.grad is not None and not torch.isfinite(p.grad).all() for p in model.parameters()):
                    raise ValueError("Non-finite training gradient")
                for group in optimizer.param_groups: group["lr"] = UnifiedTraining.learning_rate(stage, step)
                optimizer.step()
                total_steps += 1
                seconds = elapsed + time.perf_counter()-started
                if (step+1) % 8 == 0 or step+1 == stage["steps"]:
                    print(json.dumps({"stage": stage["name"], "step": step+1, "total_steps": total_steps,
                                      "loss": float(loss.detach()), "seconds": seconds}), flush=True)
                if (step+1) % 64 != 0 and step+1 != stage["steps"]: continue
                validation, objective = UnifiedTraining.validation(model, data)
                item = {"stage": stage, **counts, "validation": validation, "objective": objective,
                        "seconds": elapsed+time.perf_counter()-started, "steps": total_steps, "stage_step": step+1}
                history.append(item)
                if objective > best_objective:
                    best_objective = objective
                    UnifiedTraining.save({"model": model.state_dict(), "data_fingerprint": data.manifest["fingerprint"],
                                          "stage": stage, "validation": validation, "counts": counts,
                                          "steps": total_steps}, output / "selected.pt")
                UnifiedTraining.save({"model": model.state_dict(), "optimizer": optimizer.state_dict(), "stage": stage,
                                      "stage_index": stage_index, "stage_step": step+1, "best_objective": best_objective,
                                      "data_fingerprint": data.manifest["fingerprint"], "steps": total_steps, "seen": sorted(seen),
                                      "rng": torch.get_rng_state(), "numpy_rng": rng.bit_generator.state,
                                      "history": history, "seconds": elapsed+time.perf_counter()-started}, output / "checkpoint.pt")
                (output / "history.json").write_text(json.dumps(history, indent=2)+"\n")
                print("SELECTION", json.dumps(item), flush=True)
            optimizer.zero_grad(set_to_none=True)
            del optimizer
            gc.collect()
        report = {"history": history, "seconds": elapsed+time.perf_counter()-started, "fingerprint": data.manifest["fingerprint"],
                  "best_validation_objective": best_objective, "steps": total_steps, "sample_presentations": total_steps*batch_size,
                  "distinct_training_rows_seen": len(seen), "one_shared_backbone": True, "automatic_promotion": False,
                  "deployment_ready": False, "validation_selection_rows_per_task": 64,
                  "sampling": "Equal task/class with replacement; counterfactual pairs; not full corpus epochs",
                  "notice": "Research fine-tune. No test/calibration predictions consumed during training."}
        (output / "training-report.json").write_text(json.dumps(report, indent=2)+"\n")
        return report
