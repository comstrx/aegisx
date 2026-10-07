"""Publisher-labeled benign HTTP captures; import URI/body only, never headers or replay."""
import hashlib
import json
from pathlib import Path
from urllib.parse import urlsplit


class Wcp:
    @staticmethod
    def examples ( directory, split="train", limit=512 ):
        manifest = json.loads((directory / "manifest.json").read_text())
        examples, digests = [], {}
        for entry in manifest["members"]:
            if entry["split"] != split: continue
            source = directory / Path(entry["name"]).name
            if source.is_symlink() or not source.resolve().is_relative_to(directory.resolve()): raise ValueError("Unsafe source path")
            raw = source.read_bytes()
            if len(raw) > 16 * 1024 * 1024 or hashlib.sha256(raw).hexdigest() != entry["sha256"]: raise ValueError("WCP source digest/budget mismatch")
            digests["WCP:" + entry["name"]] = entry["sha256"]
            rows = {}
            for request in json.loads(raw):
                url, body = request.get("url"), request.get("data", "")
                if not isinstance(url, str) or not isinstance(body, str): continue
                parts = urlsplit(url)
                # Fragment/userinfo/host/header credentials do not enter the content classifier.
                target = (parts.path or "/") + ("?" + parts.query if parts.query else "")
                payload = (target + body).encode("utf-8")
                if len(payload) > 65536: continue
                rows[hashlib.sha256(payload).digest()] = payload
            selected = sorted(rows)[:limit]
            examples.extend((rows[key], 0, "wcp_benign", entry["name"]) for key in selected)
        return examples, digests
