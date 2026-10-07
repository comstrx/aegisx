"""Frozen CodeBERT transfer teacher. Original labels always supervise the student."""
import copy
import hashlib
import json
import os
import time

import numpy as np
import torch
from torch import nn
from torch.nn import functional as F

from ..metrics import Metrics
from .corpus import Corpus


class Teacher:
    @staticmethod
    def head ():
        return nn.Sequential(nn.LayerNorm(1808), nn.Linear(1808, 512), nn.GELU(), nn.Dropout(.2),
                             nn.Linear(512, 128), nn.GELU(), nn.Dropout(.1), nn.Linear(128, 1))

    @staticmethod
    def select ( data ):
        sets = []
        for partition, cap in zip(data.split(), (3000, 750, 750), strict=True):
            members = []
            for label in (0, 1):
                available = np.flatnonzero(partition & (data.origins != "recorded_lab") & (data.labels == label))
                # Selection depends only on grouping/input identity, never current model scores.
                order = sorted(available, key=lambda i: hashlib.sha256(
                    data.groups[i].encode() + data.arrays["text"][i, 0].tobytes()).digest())
                members.extend(order[:cap])
            sets.append(np.asarray(sorted(members)))
        return sets

    @staticmethod
    def prepare ( directory, output, checkpoint ):
        os.environ["HF_HUB_OFFLINE"] = "1"
        os.environ["HF_HUB_DISABLE_TELEMETRY"] = "1"
        from transformers import AutoModel, AutoTokenizer
        torch.set_num_threads(2)
        data = Corpus(directory)
        sets = Teacher.select(data)
        indices = np.unique(np.concatenate(sets))
        output.mkdir(parents=True, exist_ok=True)
        manifest_path = output / "embedding-manifest.json"
        if manifest_path.exists():
            manifest = json.loads(manifest_path.read_text())
            if manifest["fingerprint"] != data.manifest["fingerprint"]: raise ValueError("Teacher corpus changed")
            return
        tokenizer = AutoTokenizer.from_pretrained(checkpoint, local_files_only=True)
        base = AutoModel.from_pretrained(checkpoint, local_files_only=True, use_safetensors=False, weights_only=True, trust_remote_code=False).eval()
        parameters = sum(p.numel() for p in base.parameters())
        # This is an offline, pinned transfer reference; no remote code or service.
        encoder = torch.ao.quantization.quantize_dynamic(base, {nn.Linear}, dtype=torch.qint8)
        del base
        embeddings = np.lib.format.open_memmap(output / "embeddings.npy", mode="w+", dtype=np.float32, shape=(len(indices), 1536))
        texts = {}
        for i in indices:
            tokens = data.arrays["text"][i, 0]
            texts[int(i)] = bytes((tokens[tokens > 0]-1).astype(np.uint8)).decode(errors="replace")
        ordered = sorted(range(len(indices)), key=lambda k: len(texts[int(indices[k])]))
        start = time.perf_counter()
        with torch.inference_mode():
            for offset in range(0, len(ordered), 8):
                slots = ordered[offset:offset+8]
                batch = tokenizer([texts[int(indices[slot])] for slot in slots], max_length=128,
                                  truncation=True, padding=True, return_tensors="pt")
                hidden = encoder(**batch).last_hidden_state
                mask = batch["attention_mask"].unsqueeze(-1)
                pooled = (hidden * mask).sum(1) / mask.sum(1).clamp(min=1)
                embeddings[slots] = torch.cat((hidden[:, 0], pooled), dim=-1).numpy()
                if offset % 256 == 0:
                    print(json.dumps({"embedding_rows": offset, "total": len(indices), "seconds": time.perf_counter()-start}), flush=True)
        embeddings.flush()
        np.save(output / "indices.npy", indices)
        manifest_path.write_text(json.dumps({"fingerprint": data.manifest["fingerprint"], "rows": len(indices),
            "splits": [len(values) for values in sets], "frozen_pretrained_parameters": parameters,
            "encoder": "microsoft/codebert-base", "encoder_revision": "3b0952feddeffad0063f274080e3c23d75e7eb39",
            "encoder_precision": "dynamic int8 Linear; float embedding/attention", "max_tokens": 128,
            "input": "Request prefix only; CLS and masked mean; no teacher journey claim",
            "seconds": time.perf_counter()-start}, indent=2)+"\n")

    @staticmethod
    def run ( directory, cache, output ):
        torch.set_num_threads(2)
        torch.manual_seed(901)
        data = Corpus(directory)
        manifest = json.loads((cache / "embedding-manifest.json").read_text())
        if manifest["fingerprint"] != data.manifest["fingerprint"]: raise ValueError("Teacher corpus changed")
        indices = np.load(cache / "indices.npy")
        positions = {int(index): i for i, index in enumerate(indices)}
        sets = [np.asarray([positions[int(index)] for index in values]) for values in Teacher.select(data)]
        features = data.arrays["features"][indices]
        content = np.concatenate((features[:, 16:32], features[:, 40:]), axis=-1)
        x = torch.from_numpy(np.concatenate((np.load(cache / "embeddings.npy"), content), axis=-1))
        y = torch.from_numpy(data.labels[indices].astype(np.float32)).reshape(-1, 1)
        head = Teacher.head()
        optimizer = torch.optim.AdamW(head.parameters(), lr=.0003, weight_decay=.03)
        history, best, best_loss, stale = [], None, float("inf"), 0
        started = time.perf_counter()
        for epoch in range(1, 41):
            head.train()
            train = sets[0].copy()
            np.random.default_rng(901+epoch).shuffle(train)
            for offset in range(0, len(train), 64):
                chosen = train[offset:offset+64]
                optimizer.zero_grad(set_to_none=True)
                loss = F.binary_cross_entropy_with_logits(head(x[chosen]), y[chosen])
                loss.backward()
                nn.utils.clip_grad_norm_(head.parameters(), 1)
                optimizer.step()
            head.eval()
            with torch.no_grad():
                logits = head(x[sets[1]])
                value = float(F.binary_cross_entropy_with_logits(logits, y[sets[1]]))
                scores = torch.sigmoid(logits).flatten().numpy()
            item = {"epoch": epoch, "validation_loss": value, "recall_at_1pct": Metrics.report(
                y[sets[1]].numpy(), scores, Metrics.calibrate(y[sets[1]].numpy(), scores, .01))}
            history.append(item)
            print(json.dumps(item), flush=True)
            if value < best_loss-1e-5:
                best, best_loss, stale, best_epoch = copy.deepcopy(head.state_dict()), value, 0, epoch
            else: stale += 1
            if stale >= 6: break
        head.load_state_dict(best)
        head.eval()
        with torch.no_grad():
            scores = torch.cat([head(x[i:i+128]) for i in range(0, len(x), 128)]).flatten().numpy()
        output.mkdir(parents=True, exist_ok=True)
        torch.save({"head": best, "manifest": manifest, "best_epoch": best_epoch}, output / "selected.pt")
        soft = np.full(len(data.labels), np.nan, dtype=np.float32)
        soft[indices] = scores
        np.save(output / "teacher-logits.npy", soft)
        probabilities = torch.sigmoid(torch.from_numpy(scores)).numpy()
        report = {"kind": "frozen_pretrained_transfer_teacher", "frozen_pretrained_parameters": manifest["frozen_pretrained_parameters"],
            "newly_trained_parameters": sum(p.numel() for p in head.parameters()),
            "total_parameters": manifest["frozen_pretrained_parameters"] + sum(p.numel() for p in head.parameters()),
            "seconds": time.perf_counter()-started, "history": history, "best_epoch": best_epoch,
            "embedding_manifest": manifest, "deployment_ready": False, "validation": {}}
        for budget in (.01, .001):
            threshold = Metrics.calibrate(y[sets[1]].numpy(), probabilities[sets[1]], budget)
            report["validation"][str(budget)] = Metrics.report(y[sets[1]].numpy(), probabilities[sets[1]], threshold)
        # Final test is intentionally deferred until every candidate/selection rule is frozen.
        (output / "training-report.json").write_text(json.dumps(report, indent=2)+"\n")
        return report
