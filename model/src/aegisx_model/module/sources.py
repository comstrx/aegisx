"""Attributed offline examples. Publisher attack dictionaries are not production ground truth."""
import csv
import hashlib
import json
import re
from urllib.parse import quote_from_bytes
from collections import Counter

from .content import Content
from .data import Dataset
from .features import Features


class Sources:

    @staticmethod
    def canonical ( payload ):
        text = Content.decode(Content.decode(payload)).lower()
        text = re.sub(rb"[0-9]+", b"0", text)
        return re.sub(rb"\s+", b" ", text).strip()

    @staticmethod
    def http_params ( path, extra=None, benign=None, augment=False, security=None ):
        examples = []
        with path.open(newline="") as stream:
            for item in csv.DictReader(stream):
                examples.append((item["payload"].encode("utf-8"), {"norm": 0, "anom": 1}[item["label"]], "http_params"))
        provenance = {"HttpParamsDataset": hashlib.sha256(path.read_bytes()).hexdigest()}
        if extra is not None:
            manifest = json.loads((extra / "manifest.json").read_text())
            for name, expected in manifest["files"].items():
                source = extra / name
                if source.is_symlink() or not source.resolve().is_relative_to(extra.resolve()): raise ValueError("Unsafe source path")
                raw = source.read_bytes()
                if hashlib.sha256(raw).hexdigest() != expected: raise ValueError("Source digest mismatch: " + name)
                provenance["FuzzDB:" + name] = expected
                for line in raw.splitlines():
                    line = line.strip()
                    if len(line) >= 8 and not line.startswith((b"#", b"//")):
                        examples.append((line, 1, "fuzzdb"))
        if security is not None:
            manifest = json.loads((security / "manifest.json").read_text())
            for name, expected in manifest["files"].items():
                path = security / name
                if path.is_symlink() or not path.resolve().is_relative_to(security.resolve()): raise ValueError("Unsafe source path")
                raw = path.read_bytes()
                if len(raw) > 2 * 1024 * 1024 or hashlib.sha256(raw).hexdigest() != expected: raise ValueError("Security source digest mismatch")
                provenance["SecLists:" + name] = expected
                family = "xss" if "XSS/" in name else "sqli" if "SQLi/" in name else "ldap" if "LDAP" in name else "command"
                for line in raw.splitlines():
                    line = line.strip()
                    if 8 <= len(line) <= 16384 and not line.startswith((b"#", b"//")):
                        examples.append((line, 1, "seclists_" + family))
        if augment:
            expanded = []
            for payload, label, origin, *_ in examples:
                owner = "augmentation:" + hashlib.sha256(Sources.canonical(payload)).hexdigest()
                expanded.append((payload, label, origin, owner))
                expanded.append((b"/api/search?q=" + quote_from_bytes(payload, safe="").encode() + b"&page=1", label, origin + "_query", owner))
                body = json.dumps({"value": payload.decode("utf-8", errors="replace"), "source": "web"}, ensure_ascii=False).encode()
                expanded.append((b"/api/submit" + body, label, origin + "_json", owner))
            examples = expanded
        if benign is not None:
            from .wcp import Wcp
            rows, digests = Wcp.examples(benign)
            examples.extend(rows)
            provenance.update(digests)
        data = Sources.build(examples, provenance, "http_corpus" if extra or benign or security else "http_params")
        data.provenance["augmentation"] = "class-independent URI and JSON envelopes; parent template remains in one split" if augment else "none"
        return data

    @staticmethod
    def build ( examples, provenance, source ):
        content, schema = Content(), Features()
        # Conflicting exact publisher labels are quarantined, not resolved using a prediction.
        payload_labels = {}
        for example in examples: payload_labels.setdefault(example[0], set()).add(example[1])
        rows, labels, canonical, origins, owners = [], [], [], [], []
        seen = set()
        for example in examples:
            payload, label, origin = example[:3]
            if payload in seen or len(payload_labels[payload]) != 1: continue
            seen.add(payload)
            rows.append(content.row(payload[:16384], len(payload)))
            labels.append(label)
            canonical.append(hashlib.sha256(Sources.canonical(payload)).hexdigest())
            origins.append(origin)
            owners.append(example[3] if len(example) > 3 else None)
        # Merge templates AND identical inputs across all sources before splitting.
        parents = list(range(len(rows)))
        def root ( index ):
            while parents[index] != index:
                parents[index] = parents[parents[index]]
                index = parents[index]
            return index
        features = schema.normalize(rows)
        signatures = {}
        for index, row in enumerate(features):
            keys = ["t:" + canonical[index], "f:" + hashlib.sha256(row.astype("<f4").tobytes()).hexdigest()]
            if owners[index] is not None: keys.append("capture:" + owners[index])
            for signature in keys:
                if signature in signatures: parents[root(index)] = root(signatures[signature])
                else: signatures[signature] = index
        groups = [f"http-corpus:{root(index)}" for index in range(len(rows))]
        data = Dataset(features, labels, groups, source, normalized=True)
        data.origins = origins
        data.signatures = sorted(signatures)
        data.provenance = {"digests": provenance, "raw_rows": len(examples), "rows": len(rows),
            "origins": dict(Counter(origins)), "conflicting_payloads_removed": sum(len(value) > 1 for value in payload_labels.values()),
            "grouping": "connected components of capture sessions, canonical templates and identical normalized inputs across sources",
            "missing": "No observed actor history, response, backend logs or timing in these sources",
            "labels": "Publisher labels and attack dictionary membership; context-dependent, not model-generated ground truth"}
        return data
