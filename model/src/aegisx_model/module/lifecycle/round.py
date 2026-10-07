"""Three registered candidates, grouped validation and training-only distillation."""
import copy
import json
import time

import numpy as np
import torch
from torch.nn import functional as F

from ..metrics import Metrics
from .corpus import Corpus
from .network import LifecycleNetwork
from .train import Training


class Round:
    @staticmethod
    def validation ( model, data, sets ):
        result, objective = {}, 0.0
        for task, indices in sets.items():
            labels = data.labels[indices[1]]
            scores = Training.scores(model, data, indices[1], task)
            clipped = np.clip(scores, 1e-6, 1-1e-6)
            loss = float(-.5 * (np.log(clipped[labels == 1]).mean() + np.log1p(-clipped[labels == 0]).mean()))
            reports = {}
            for budget in (.01, .001):
                threshold = Metrics.calibrate(labels, scores, budget)
                reports[str(budget)] = Metrics.report(labels, scores, threshold)
            # Fixed low-FPR recall objective, with class-balanced loss as a small tie breaker.
            objective += .5 * (reports["0.01"]["recall"] + reports["0.001"]["recall"]) - .02 * loss
            result[task] = {"budgets": reports, "balanced_loss": loss}
        return result, objective

    @staticmethod
    def run ( directory, output, teacher=None, epochs=16 ):
        torch.set_num_threads(2)
        torch.manual_seed(902)
        data = Corpus(directory)
        partitions = data.split()
        tasks = {"content": data.origins != "recorded_lab", "journey": data.origins == "recorded_lab"}
        sets = {task: [np.flatnonzero(mask & task_mask) for mask in partitions] for task, task_mask in tasks.items()}
        model = LifecycleNetwork(variant="compact")
        model.feature_mask.copy_(torch.from_numpy(np.any(data.arrays["features"][partitions[0]] != 0, axis=0).astype(np.float32)))
        optimizer = torch.optim.AdamW(model.parameters(), lr=.0006, weight_decay=.025, foreach=True)
        soft = np.load(teacher / "teacher-logits.npy") if teacher else None
        if soft is not None:
            report = json.loads((teacher / "training-report.json").read_text())
            if report["embedding_manifest"]["fingerprint"] != data.manifest["fingerprint"]:
                raise ValueError("Distillation corpus mismatch")
        history, best, best_objective, stale = [], None, -float("inf"), 0
        output.mkdir(parents=True, exist_ok=True)
        started = time.perf_counter()
        for epoch in range(1, epochs+1):
            model.train()
            losses = {}
            for task, indices in sets.items():
                train = indices[0].copy()
                np.random.default_rng(902+epoch).shuffle(train)
                labels = data.labels[train]
                positive_weight = float((len(labels)-labels.sum()) / max(1, labels.sum()))
                total = 0.0
                for offset in range(0, len(train), 64):
                    selected = train[offset:offset+64]
                    batch = Training.tensors(data, selected)
                    target = torch.from_numpy(data.labels[selected].astype(np.float32)).reshape(-1, 1)
                    optimizer.zero_grad(set_to_none=True)
                    logits = model.content_logits(batch["features"], batch["text"]) if task == "content" else model.journey_logits(**batch)
                    # A small symmetric smoothing prior avoids confidently saturated probabilities.
                    loss = F.binary_cross_entropy_with_logits(logits, target*.98+.01,
                                                              pos_weight=torch.tensor(positive_weight))
                    if task == "content" and soft is not None:
                        known = np.isfinite(soft[selected])
                        if known.any():
                            reference = torch.from_numpy(soft[selected][known]).reshape(-1, 1)
                            # Never distill a confidently wrong teacher pseudo-label.
                            consistent = (reference >= 0) == (target[known] >= .5)
                            if consistent.any():
                                loss = loss + .15 * 4 * F.binary_cross_entropy_with_logits(
                                    logits[known][consistent]/2, torch.sigmoid(reference[consistent]/2))
                    loss.backward()
                    torch.nn.utils.clip_grad_norm_(model.parameters(), 1)
                    optimizer.step()
                    total += float(loss.detach())*len(selected)
                losses[task] = total/len(train)
            validation, objective = Round.validation(model, data, sets)
            item = {"epoch": epoch, "training_loss": losses, "objective": objective,
                    "validation": validation, "seconds": time.perf_counter()-started}
            history.append(item)
            print(json.dumps(item), flush=True)
            if objective > best_objective + 1e-5:
                best, best_objective, stale, best_epoch = copy.deepcopy(model.state_dict()), objective, 0, epoch
            else: stale += 1
            temporary = output / "checkpoint.tmp"
            torch.save({"model": model.state_dict(), "best": best, "optimizer": optimizer.state_dict(),
                        "epoch": epoch, "best_epoch": best_epoch, "fingerprint": data.manifest["fingerprint"],
                        "history": history, "variant": "compact", "seed": 902}, temporary)
            temporary.replace(output / "checkpoint.pt")
            if stale >= 5: break
        model.load_state_dict(best)
        torch.save({"model": best, "spec": model.spec, "parameter_count": model.parameters_count(),
                    "variant": "compact"}, output / "selected.pt")
        validation, objective = Round.validation(model, data, sets)
        report = {"parameter_count": model.parameters_count(), "variant": "compact", "input_schema": model.spec,
                  "best_epoch": best_epoch, "epochs": len(history), "seconds": time.perf_counter()-started,
                  "history": history, "validation": validation, "selection_objective": objective,
                  "data": data.manifest, "splits": {task: [len(s) for s in indices] for task, indices in sets.items()},
                  "distillation": teacher is not None, "deployment_ready": False,
                  "notice": "Same grouped corpus; final tests deferred until candidate selection. Teacher is request-only and frozen pretrained."}
        (output / "training-report.json").write_text(json.dumps(report, indent=2)+"\n")
        return report
