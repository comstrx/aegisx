"""Verified public text plus recorded application journeys with group separation."""
import csv
import hashlib
import json
from collections import Counter
from pathlib import Path
from urllib.parse import quote_from_bytes

import numpy as np

from ..sources import Sources
from ..wcp import Wcp
from .inputs import Inputs
from .integrity import Integrity


class Corpus:
    @staticmethod
    def public ( root ):
        items, provenance = [], {}
        manifest = json.loads((root / "sources.json").read_text())
        path = root / "data/sources/payload_full.csv"
        raw = path.read_bytes()
        if hashlib.sha256(raw).hexdigest() != manifest["http_params"]["files"]["payload_full.csv"]["sha256"]:
            raise ValueError("HttpParams digest mismatch")
        provenance["HttpParams"] = hashlib.sha256(raw).hexdigest()
        by_label = {0: [], 1: []}
        with path.open(newline="") as stream:
            for row in csv.DictReader(stream):
                label = {"norm": 0, "anom": 1}[row["label"]]
                by_label[label].append(row["payload"].encode())
        for label, values in by_label.items():
            for payload in sorted(set(values), key=lambda x: hashlib.sha256(x).digest())[:4000]:
                items.append((payload, label, "http_params", None))
        for source in ("fuzzdb", "seclists"):
            directory = root / "data" / source
            hashes = json.loads((directory / "manifest.json").read_text())["files"]
            rows = {}
            for name, expected in hashes.items():
                path = directory / name
                if path.is_symlink() or not path.resolve().is_relative_to(directory.resolve()):
                    raise ValueError("Unsafe source path")
                data = path.read_bytes()
                if len(data) > 2 * 1024 * 1024 or hashlib.sha256(data).hexdigest() != expected:
                    raise ValueError("Source digest/budget mismatch")
                provenance[source + ":" + name] = expected
                for line in data.splitlines():
                    line = line.strip()
                    if 8 <= len(line) <= 16384 and not line.startswith((b"#", b"//")):
                        rows[line] = (line, 1, source, None)
            items.extend(rows[key] for key in sorted(rows, key=lambda x: hashlib.sha256(x).digest())[:4096])
        benign, digests = Wcp.examples(root / "data/wcp", limit=192)
        provenance.update(digests)
        items.extend(benign)
        records = []
        for payload, label, origin, owner in items:
            signature = hashlib.sha256(Sources.canonical(payload)).hexdigest()
            choice = int(signature[:2], 16) % 3
            # Class-independent packaging; no fabricated response or journey.
            if origin != "wcp_benign":
                if choice == 1: payload = b"GET /api/search?q=" + quote_from_bytes(payload, safe="").encode() + b"\n"
                elif choice == 2: payload = b"POST /api/submit\n" + json.dumps({"value": payload.decode(errors="replace")}, ensure_ascii=False).encode()
            records.append({"request": payload.decode(errors="replace"), "label": label, "origin": origin,
                            "group": "template:" + signature, "owner": owner})
        return records, provenance

    @staticmethod
    def build ( records, directory, provenance ):
        directory.mkdir(parents=True, exist_ok=True)
        # Exact conflicting observations are removed; template/capture families remain connected.
        parents = list(range(len(records)))
        def root ( index ):
            while parents[index] != index:
                parents[index] = parents[parents[index]]
                index = parents[index]
            return index
        signatures, labels_by_input, encoded = {}, {}, []
        schema = Inputs()
        for index, record in enumerate(records):
            tensors = schema.record(record)
            signature = hashlib.sha256(b"".join(value.tobytes() for value in tensors.values())).hexdigest()
            labels_by_input.setdefault(signature, set()).add(record["label"])
            encoded.append((tensors, signature))
            keys = ["group:" + record["group"], "input:" + signature]
            if record.get("owner"): keys.append("capture:" + record["owner"])
            for key in keys:
                if key in signatures: parents[root(index)] = root(signatures[key])
                else: signatures[key] = index
        components = {}
        for index, record in enumerate(records): components.setdefault(root(index), []).append(record["group"])
        owners = {key: min(values) for key, values in components.items()}
        keep = [i for i, (_, sig) in enumerate(encoded) if len(labels_by_input[sig]) == 1]
        if not keep: raise ValueError("Empty lifecycle corpus")
        sample = encoded[keep[0]][0]
        arrays = {name: np.lib.format.open_memmap(directory / (name + ".npy"), mode="w+",
                  dtype=np.uint16 if value.dtype == np.int64 else np.float32, shape=(len(keep), *value.shape))
                  for name, value in sample.items()}
        labels, groups, origins = [], [], []
        seen = []
        for offset, index in enumerate(keep):
            record = records[index]
            for name, value in encoded[index][0].items(): arrays[name][offset] = value
            labels.append(record["label"])
            groups.append(owners[root(index)])
            origins.append(record["origin"])
            seen.append(encoded[index][1])
        for value in arrays.values(): value.flush()
        for name, value in (("labels", labels), ("groups", groups), ("origins", origins)):
            np.save(directory / (name + ".npy"), np.asarray(value))
        fingerprint = hashlib.sha256("".join(seen).encode()).hexdigest()
        report = {"rows": len(keep), "origins": dict(Counter(origins)), "groups": len(set(groups)),
                  "conflicting_rows_removed": len(records) - len(keep), "fingerprint": fingerprint,
                  "provenance": provenance, "input_schema": schema.spec,
                  "notice": "Public payload labels and controlled lab outcomes are different tasks. Lab traces are recorded, not production telemetry."}
        (directory / "manifest.json").write_text(json.dumps(report, indent=2) + "\n")
        return Integrity.seal(directory)

    def __init__ ( self, directory ):
        self.directory = Path(directory)
        manifest = json.loads((self.directory / "manifest.json").read_text())
        Integrity.verify(self.directory, manifest)
        self.arrays = {name: np.load(self.directory / (name + ".npy"), mmap_mode="r")
                       for name in ("features", "text", "event_text", "event_values", "coverage")}
        self.labels = np.load(self.directory / "labels.npy")
        self.groups = np.load(self.directory / "groups.npy")
        self.origins = np.load(self.directory / "origins.npy")
        self.manifest = json.loads((self.directory / "manifest.json").read_text())

    def split ( self ):
        buckets = np.asarray([int(hashlib.sha256(("split-42:" + name).encode()).hexdigest()[:8], 16) % 10 for name in self.groups])
        masks = [buckets < 6, (buckets >= 6) & (buckets < 8), buckets >= 8]
        for mask in masks:
            if len(np.unique(self.labels[mask])) != 2: raise ValueError("Every split needs both classes")
        return masks

    def batch ( self, indices ):
        return {name: np.asarray(array[indices], dtype=np.int64 if name in ("text", "event_text") else np.float32)
                for name, array in self.arrays.items()}
