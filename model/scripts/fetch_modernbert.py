"""Fetch an immutable official encoder and tokenizer; never execute repository code."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import urllib.request

ROOT = Path(__file__).resolve().parents[1]


def verify ( path, expected ):
    sha = hashlib.sha256()
    blob = hashlib.sha1(f"blob {expected['bytes']}\0".encode())
    with path.open("rb") as stream:
        while chunk := stream.read(1024 * 1024):
            sha.update(chunk)
            blob.update(chunk)
    size = path.stat().st_size
    if size != expected["bytes"] or (expected["sha256"] and sha.hexdigest() != expected["sha256"]) or (
            not expected["sha256"] and blob.hexdigest() != expected["git_blob"]):
        raise ValueError(f"Pinned artifact integrity mismatch: {path.name}")
    return {"bytes": size, "sha256": sha.hexdigest()}


def fetch ( source_path, directory ):
    source = json.loads(source_path.read_text())
    directory.mkdir(parents=True, exist_ok=True)
    verified = {}
    for name, expected in source["files"].items():
        if Path(name).name != name: raise ValueError("Artifact must be a direct child")
        target, temporary = directory / name, directory / (name + ".download")
        if not target.exists():
            try:
                url = f'https://huggingface.co/{source["repository"]}/resolve/{source["revision"]}/{name}'
                with urllib.request.urlopen(url, timeout=120) as response, temporary.open("wb") as stream:
                    size = 0
                    while chunk := response.read(1024 * 1024):
                        size += len(chunk)
                        if size > expected["bytes"]: raise ValueError("Download budget exceeded")
                        stream.write(chunk)
                    stream.flush()
                    os.fsync(stream.fileno())
                verify(temporary, expected)
                temporary.replace(target)
            finally:
                temporary.unlink(missing_ok=True)
        verified[name] = verify(target, expected)
        print(name, verified[name], flush=True)
    (directory / "manifest.json").write_text(json.dumps(source | {"verified": verified}, indent=2) + "\n")


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--source", type=Path, default=ROOT / "modernbert-source.json")
    parser.add_argument("--output", type=Path, default=ROOT / "data/modernbert")
    options = parser.parse_args()
    fetch(options.source, options.output)
