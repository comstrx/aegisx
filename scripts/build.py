"""Build the embedded Next.js panel, then the single Linux executable."""

import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import tomllib

root = Path(__file__).resolve().parents[1]
parser = argparse.ArgumentParser()
parser.add_argument("--debug", action="store_true")
options = parser.parse_args()
environment = os.environ.copy()
node_version = (root / "panel/.nvmrc").read_text().strip()
local_node = root / f"panel/.runtime/node-v{node_version}-linux-x64/bin"
environment["PATH"] = os.pathsep.join([str(local_node), str(Path.home() / ".cargo/bin"), environment.get("PATH", "")])
environment["NEXT_TELEMETRY_DISABLED"] = "1"
version = subprocess.check_output(["node", "--version"], env=environment, text=True).strip()
if version != "v" + node_version:
    raise SystemExit(f"Install Node {node_version} and select it in PATH (found {version})")
for command, directory in [
    (["npm", "ci", "--no-audit", "--no-fund"], root / "panel"),
    (["npm", "run", "build"], root / "panel"),
    (["cargo", "build", "--locked"] + ([] if options.debug else ["--release"]), root / "server"),
]:
    subprocess.run(command, cwd=directory, env=environment, check=True)
profile = "debug" if options.debug else "release"
binary = root / f"server/target/{profile}/aegisx"
destination = root / "dist"
destination.mkdir(exist_ok=True)
shutil.copy2(binary, destination / "aegisx")
for name in ["DATASET-LICENSE.txt", "FUZZDB-NOTICE.txt", "WCP-LICENSE.txt", "WCP-NOTICE.txt", "SECLISTS-LICENSE.txt", "SECLISTS-NOTICE.txt", "CODEBERT-NOTICE.txt"]:
    shutil.copy2(root / "model/weights" / name, destination / name)
shutil.copy2(root / "server/MIMALLOC-NOTICE.txt", destination / "MIMALLOC-NOTICE.txt")
shutil.copy2(root / "server/vendor/pingora-core/LICENSE", destination / "PINGORA-LICENSE.txt")
shutil.copy2(root / "server/vendor/pingora-core/AEGISX-PATCH.md", destination / "PINGORA-PATCH-NOTICE.md")
shutil.copy2(root / "server/vendor/pingora-timeout/AEGISX-PATCH.md", destination / "PINGORA-TIMEOUT-PATCH-NOTICE.md")
manifest = {
    "binary": "aegisx", "version": tomllib.loads((root / "server/Cargo.toml").read_text())["package"]["version"],
    "rust_toolchain": tomllib.loads((root / "rust-toolchain.toml").read_text())["toolchain"]["channel"],
    "profile": profile, "bytes": binary.stat().st_size,
    "sha256": hashlib.sha256(binary.read_bytes()).hexdigest(),
    "pingora_core_patch_sha256": hashlib.sha256((root / "server/vendor/pingora-core/src/protocols/http/v1/server.rs").read_bytes()).hexdigest(),
    "pingora_timeout_patch_sha256": hashlib.sha256((root / "server/vendor/pingora-timeout/src/lib.rs").read_bytes()).hexdigest(),
    "node_build_version": version, "model": json.loads((root / "model/weights/metadata.json").read_text()),
}
(destination / "manifest.json").write_text(json.dumps(manifest, indent=2) + "\n")
print(json.dumps(manifest, indent=2))
