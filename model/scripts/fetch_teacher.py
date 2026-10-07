"""Fetch the pinned public Microsoft CodeBERT checkpoint without remote code."""
import hashlib
import json
import os
import urllib.request
from pathlib import Path

REVISION = "3b0952feddeffad0063f274080e3c23d75e7eb39"
EXPECTED = {
    "pytorch_model.bin": (498627950, "28b61fd8fa069f6bc966f4cb9572a4026ab2a784fca8fb224020d91b744e32d6"),
    "config.json": (498, "742474d6148a061fa975dadc09ce93725fa9673c55421ed316ef75532cc6b7aa"),
    "vocab.json": (898822, "06b4d46c8e752d410213d9548eb27a54db70fda0319b6271fb8d59dead5e1cab"),
    "merges.txt": (456318, "1ce1664773c50f3e0cc8842619a93edc4624525b728b188a9e0be33b7726adc5"),
    "tokenizer_config.json": (25, "994f46754c5bf4014f1aa92d34b1374319c3a6b3f702105cd5b742beaecd18ce"),
    "special_tokens_map.json": (150, "7638f5bbbe86ef6d604ef28ad3647dc690d6d117c81c0d63e885416be8da1150"),
}
ROOT = Path(__file__).resolve().parents[1] / "data/codebert"
FILES = ("config.json", "vocab.json", "merges.txt", "tokenizer_config.json", "special_tokens_map.json", "pytorch_model.bin")
ROOT.mkdir(parents=True, exist_ok=True)
manifest = {"repository": "microsoft/codebert-base", "revision": REVISION, "files": {}}
for name in FILES:
    target = ROOT / name
    expected = EXPECTED.get(name)
    if target.exists():
        digest = hashlib.sha256(target.read_bytes()).hexdigest()
        if expected and (target.stat().st_size, digest) != expected:
            raise ValueError(f"Existing checkpoint digest mismatch: {name}")
    else:
        limit = expected[0] if expected else 4 * 1024 * 1024
        temporary = target.with_suffix(".download")
        digest, size = hashlib.sha256(), 0
        try:
            url = f"https://huggingface.co/microsoft/codebert-base/resolve/{REVISION}/{name}"
            with urllib.request.urlopen(url, timeout=120) as response, temporary.open("wb") as stream:
                while chunk := response.read(1024 * 1024):
                    size += len(chunk)
                    if size > limit: raise ValueError("Download budget exceeded")
                    digest.update(chunk)
                    stream.write(chunk)
                stream.flush()
                os.fsync(stream.fileno())
            if expected and (size, digest.hexdigest()) != expected: raise ValueError("Checkpoint digest mismatch")
            temporary.replace(target)
        finally:
            temporary.unlink(missing_ok=True)
        digest = digest.hexdigest()
    manifest["files"][name] = {"bytes": target.stat().st_size, "sha256": digest}
    print(name, manifest["files"][name], flush=True)
notice_revision = "c0de43d3aaf38e89290f1efb771f8de845e7a489"
for name, expected in {
    "LICENSE": "7c77a44a8acd9b41fdc209864a8016b3d430b5d0e09309818d5b7444336df744",
    "NOTICE.md": "39488d1b29ec11c7d812f0cd70fac4528d7bc9f09852403d1285b1248d8e8c17",
}.items():
    target = ROOT / name
    if target.exists(): raw = target.read_bytes()
    else:
        url = f"https://raw.githubusercontent.com/microsoft/CodeBERT/{notice_revision}/{name}"
        with urllib.request.urlopen(url, timeout=30) as response: raw = response.read(262145)
    if len(raw) > 262144 or hashlib.sha256(raw).hexdigest() != expected:
        raise ValueError("Teacher notice digest mismatch")
    target.write_bytes(raw)
    manifest["files"][name] = {"bytes": len(raw), "sha256": expected}
manifest["license"] = {"repository": "https://github.com/microsoft/CodeBERT", "commit": notice_revision, "license": "MIT"}
(ROOT / "manifest.json").write_text(json.dumps(manifest, indent=2) + "\n")
