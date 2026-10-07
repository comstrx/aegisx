"""Two distinct objectives: public request-content risk and recorded journey misuse."""
import copy
import json
import time

import numpy as np
import torch
from torch.nn import functional as F

from ..metrics import Metrics
from .corpus import Corpus
from .network import LifecycleNetwork


class Training:
    @staticmethod
    def tensors ( data, indices ):
        return {name: torch.from_numpy(value) for name, value in data.batch(indices).items()}

    @staticmethod
    def scores ( model, data, indices, task, ablate=None ):
        values = []
        model.eval()
        with torch.no_grad():
            for start in range(0, len(indices), 128):
                batch = Training.tensors(data, indices[start:start + 128])
                if ablate == "events":
                    batch["event_text"].zero_()
                    batch["event_values"].zero_()
                if ablate == "response":
                    batch["text"][:, 1].zero_()
                    batch["coverage"][:, 3].zero_()
                if ablate == "request": batch["text"][:, 0].zero_()
                if ablate == "numeric": batch["features"].zero_()
                logits = model.content_logits(batch["features"], batch["text"]) if task == "content" else model.journey_logits(**batch)
                values.extend(torch.sigmoid(logits).flatten().tolist())
        return np.asarray(values, dtype=np.float32)

    @staticmethod
    def run ( directory, output, epochs=12 ):
        torch.set_num_threads(2)
        torch.manual_seed(84)
        np.random.seed(84)
        data = Corpus(directory)
        partitions = data.split()
        tasks = {"content": data.origins != "recorded_lab", "journey": data.origins == "recorded_lab"}
        indices = {task: [np.flatnonzero(mask & task_mask) for mask in partitions] for task, task_mask in tasks.items()}
        model = LifecycleNetwork()
        model.feature_mask.copy_(torch.from_numpy(np.any(data.arrays["features"][partitions[0]] != 0, axis=0).astype(np.float32)))
        optimizer = torch.optim.AdamW(model.parameters(), lr=.0003, weight_decay=.02, foreach=True)
        best, best_loss, stale, history = None, float("inf"), 0, []
        output.mkdir(parents=True, exist_ok=True)
        start = time.perf_counter()
        for epoch in range(1, epochs + 1):
            model.train()
            epoch_start = time.perf_counter()
            losses = {}
            # Separate content pass avoids evaluating the large journey branch on absent traces.
            for task, sets in indices.items():
                train = sets[0].copy()
                np.random.default_rng(84 + epoch).shuffle(train)
                labels = data.labels[train]
                positive_weight = float((len(labels) - labels.sum()) / max(1, labels.sum()))
                total = 0.0
                for offset in range(0, len(train), 64):
                    selected = train[offset:offset + 64]
                    batch = Training.tensors(data, selected)
                    target = torch.from_numpy(data.labels[selected].astype(np.float32)).reshape(-1, 1)
                    optimizer.zero_grad(set_to_none=True)
                    logits = model.content_logits(batch["features"], batch["text"]) if task == "content" else model.journey_logits(**batch)
                    loss = F.binary_cross_entropy_with_logits(logits, target, pos_weight=torch.tensor(positive_weight))
                    loss.backward()
                    torch.nn.utils.clip_grad_norm_(model.parameters(), 1.0)
                    optimizer.step()
                    total += float(loss.detach()) * len(selected)
                losses[task] = total / len(train)
            validation, criterion = {}, 0.0
            for task, sets in indices.items():
                scores = Training.scores(model, data, sets[1], task)
                labels = data.labels[sets[1]]
                threshold = Metrics.calibrate(labels, scores, .01)
                report = Metrics.report(labels, scores, threshold)
                clipped = np.clip(scores, 1e-6, 1 - 1e-6)
                # Average each class, then each task; source volume cannot hide a failed task.
                loss = -.5 * (np.log(clipped[labels == 1]).mean() + np.log1p(-clipped[labels == 0]).mean())
                criterion += float(loss) / 2
                validation[task] = report
            item = {"epoch": epoch, "seconds": time.perf_counter() - epoch_start, "training_loss": losses,
                    "balanced_validation_loss": criterion, "validation": validation}
            history.append(item)
            print(json.dumps(item), flush=True)
            if criterion < best_loss - 1e-5:
                best_loss, stale = criterion, 0
                best = copy.deepcopy(model.state_dict())
                best_epoch = epoch
            else: stale += 1
            temporary = output / "checkpoint.tmp"
            torch.save({"model": model.state_dict(), "best": best, "optimizer": optimizer.state_dict(), "epoch": epoch,
                        "best_epoch": best_epoch, "fingerprint": data.manifest["fingerprint"], "history": history,
                        "input_schema": model.spec, "seed": 84}, temporary)
            temporary.replace(output / "checkpoint.pt")
            if stale >= 3: break
        model.load_state_dict(best)
        torch.save({"model": best, "spec": model.spec, "parameter_count": model.parameters_count()}, output / "selected.pt")
        report = {"parameter_count": model.parameters_count(), "input_schema": model.spec, "best_epoch": best_epoch,
                  "epochs": len(history), "seconds": time.perf_counter() - start, "history": history,
                  "data": data.manifest, "tasks": {}, "deployment_ready": False,
                  "notice": "Content labels and journey outcomes are separate heads. Recorded lab performance is not production journey understanding."}
        for task, sets in indices.items():
            validation = Training.scores(model, data, sets[1], task)
            threshold = Metrics.calibrate(data.labels[sets[1]], validation, .01)
            report["tasks"][task] = {"threshold": threshold, "validation": Metrics.report(data.labels[sets[1]], validation, threshold),
                                    "test": Metrics.report(data.labels[sets[2]], Training.scores(model, data, sets[2], task), threshold),
                                    "splits": [len(part) for part in sets]}
            report["tasks"][task]["ablations"] = {
                name: Metrics.report(data.labels[sets[2]], Training.scores(model, data, sets[2], task, name), threshold)
                for name in (("request", "numeric") if task == "content" else ("request", "response", "events", "numeric"))}
        (output / "training-report.json").write_text(json.dumps(report, indent=2) + "\n")
        return report
