"""Bounded unified-model tensors with fixed splits and oracle data kept out of inputs."""
import hashlib
import json
from pathlib import Path

import numpy as np
from transformers import AutoTokenizer

from .corpus import Corpus
from .inputs import Inputs
from .worlds import Worlds
from .retries import Retries


class UnifiedData:
    NAMES = ("request_ids", "request_mask", "response_ids", "response_mask",
             "event_ids", "event_mask", "event_values", "features", "coverage")

    @staticmethod
    def text ( tokens ):
        values = np.asarray(tokens)
        return bytes((values[values > 0]-1).tolist()).decode("utf-8", errors="replace")

    @staticmethod
    def stratified ( labels, indices, count, seed ):
        rng = np.random.default_rng(seed)
        return np.concatenate([rng.permutation(indices[labels[indices] == value])[:count//2] for value in (0, 1)])

    @staticmethod
    def encode ( arrays, tokenizer ):
        size = len(arrays["features"])
        encoded = {}
        coverage = np.array(arrays["coverage"], dtype=np.float32, copy=True)
        for name, stream, limit in (("request", 0, 128), ("response", 1, 64)):
            texts = [UnifiedData.text(value) for value in arrays["text"][:, stream]]
            lengths = tokenizer(texts, add_special_tokens=True, truncation=False, return_length=True)["length"]
            coverage[:, stream] = np.maximum(coverage[:, stream], np.asarray(lengths) > limit)
            result = tokenizer(texts, padding="max_length", truncation=True, max_length=limit, return_tensors="np")
            encoded[name+"_ids"] = result["input_ids"].astype(np.int32)
            encoded[name+"_mask"] = result["attention_mask"].astype(np.int32)
            encoded[name+"_mask"][np.asarray([not text for text in texts])] = 0
        names = [UnifiedData.text(value) for value in arrays["event_text"].reshape(-1, 64)]
        lengths = tokenizer(names, add_special_tokens=False, truncation=False, return_length=True)["length"]
        coverage[:, 2] = np.maximum(coverage[:, 2], (np.asarray(lengths).reshape(size, 32) > 16).any(1))
        events = tokenizer(names, padding="max_length", truncation=True, max_length=16,
                           add_special_tokens=False, return_tensors="np")
        encoded["event_ids"] = events["input_ids"].astype(np.int32).reshape(size, 32, 16)
        encoded["event_mask"] = events["attention_mask"].astype(np.int32).reshape(size, 32, 16)
        for name in ("event_values", "features"):
            encoded[name] = np.asarray(arrays[name], dtype=np.float32)
        encoded["coverage"] = coverage
        return encoded

    @staticmethod
    def prepare ( corpus, pretrained, output ):
        if (output / "manifest.json").exists(): raise ValueError("Prepared round is immutable; use a new output directory")
        output.mkdir(parents=True, exist_ok=True)
        source = Corpus(corpus)
        selected, partitions, tasks, groups = [], [], [], []
        for split, mask in enumerate(source.split()):
            for task, domain in enumerate((source.origins != "recorded_lab", source.origins == "recorded_lab")):
                ids = UnifiedData.stratified(source.labels, np.flatnonzero(mask & domain),
                                             (4096 if task == 0 else 1024) if split == 0 else (512 if task == 0 else 256),
                                             1001+split+task)
                selected.extend(ids.tolist())
                partitions.extend([(3 if split == 1 and int(hashlib.sha256(str(source.groups[i]).encode()).hexdigest()[:8], 16) % 2 else split) for i in ids])
                tasks.extend([task]*len(ids))
                groups.extend(source.groups[ids].tolist())
        legacy_count = len(selected)
        original = source.batch(selected)
        labels = source.labels[selected].tolist()
        applications = ["public-content" if task == 0 else "legacy-lab" for task in tasks]
        pairs = [""] * len(selected)
        worlds = Worlds.collect(families=12) + Retries.collect(families=12)
        schema = Inputs()
        tensors = [schema.record(row) for row in worlds]
        arrays = {name: np.concatenate((original[name], np.stack([row[name] for row in tensors]))) for name in original}
        for row in worlds:
            partitions.append({"train": 0, "validation": 1, "test": 2, "calibration": 3}[row["split"]])
            labels.append(row["label"])
            tasks.append(1)
            groups.append(row["group"])
            applications.append(row["application"])
            pairs.append(row["counterfactual"])
        tokenizer = AutoTokenizer.from_pretrained(str(pretrained), local_files_only=True, trust_remote_code=False)
        encoded = UnifiedData.encode(arrays, tokenizer)
        # Token truncation can collapse formerly distinct records. Remove ambiguous or cross-split inputs.
        signatures, members = [], {}
        for index in range(len(labels)):
            digest = hashlib.sha256(bytes([tasks[index]]) + b"".join(encoded[name][index].tobytes() for name in UnifiedData.NAMES)).hexdigest()
            signatures.append(digest)
            members.setdefault(digest, []).append(index)
        invalid = {signature for signature, indices in members.items()
                   if len({partitions[i] for i in indices}) > 1 or len({labels[i] for i in indices}) > 1}
        keep = np.asarray([i for i, signature in enumerate(signatures) if signature not in invalid])
        removed = len(labels)-len(keep)
        encoded = {name: value[keep] for name, value in encoded.items()}
        arrays = {name: value[keep] for name, value in arrays.items()}
        labels, partitions, tasks, groups, applications, pairs = ([values[i] for i in keep]
            for values in (labels, partitions, tasks, groups, applications, pairs))
        for name, value in encoded.items(): np.save(output / (name + ".npy"), value)
        for name, value in {"labels": labels, "partitions": partitions, "tasks": tasks, "groups": groups,
                            "applications": applications, "pairs": pairs}.items():
            np.save(output / (name + ".npy"), np.asarray(value))
        # Saved only for reproducible baseline evaluation; not added to model tensors.
        for name, value in arrays.items(): np.save(output / ("baseline_" + name + ".npy"), value)
        (output / "worlds.jsonl").write_text("".join(json.dumps(row) + "\n" for row in worlds))
        hashes = {p.name: hashlib.sha256(p.read_bytes()).hexdigest() for p in sorted(output.glob("*.npy"))}
        manifest = {"source_fingerprint": source.manifest["fingerprint"], "files": hashes,
                    "fingerprint": hashlib.sha256(json.dumps(hashes, sort_keys=True).encode()).hexdigest(),
                    "schema_version": 2, "coverage_semantics": "Capture OR tokenizer truncation; event flag includes event-name truncation",
                    "rows": len(labels), "selected_legacy_before_dedup": legacy_count, "world_rows_before_dedup": len(worlds), "ambiguous_or_cross_split_rows_removed": removed,
                    "token_budget": {"request": 128, "response": 64, "event_name": 16, "events": 32, "numeric_tokens": 5},
                    "pretrained": json.loads((pretrained / "manifest.json").read_text()),
                    "application_splits": {name: spec[0] for name, spec in Worlds.APPLICATIONS.items()},
                    "notice": "Applications are disjoint controlled SQLite fixtures with shared generator logic, not independent production validation. "
                              "Public content labels describe attack-like payloads; lab labels describe actual policy violations."}
        (output / "manifest.json").write_text(json.dumps(manifest, indent=2) + "\n")
        return manifest

    def __init__ ( self, directory ):
        self.directory = Path(directory)
        self.manifest = json.loads((self.directory / "manifest.json").read_text())
        fingerprint = hashlib.sha256(json.dumps(self.manifest["files"], sort_keys=True).encode()).hexdigest()
        if fingerprint != self.manifest["fingerprint"]: raise ValueError("Unified manifest fingerprint mismatch")
        for name, expected in self.manifest["files"].items():
            if hashlib.sha256((self.directory / name).read_bytes()).hexdigest() != expected:
                raise ValueError(f"Unified corpus integrity mismatch: {name}")
        self.arrays = {name: np.load(self.directory / (name + ".npy"), mmap_mode="r") for name in self.NAMES}
        for name in ("labels", "partitions", "tasks", "groups", "applications", "pairs"):
            setattr(self, name, np.load(self.directory / (name + ".npy")))
        from itertools import combinations
        for left, right in combinations(range(4), 2):
            if set(self.groups[self.partitions == left]) & set(self.groups[self.partitions == right]):
                raise ValueError("Cross-partition group leakage")

    def batch ( self, indices ):
        return {name: np.asarray(value[indices], dtype=np.int64 if "ids" in name or "mask" in name else np.float32)
                for name, value in self.arrays.items()}
