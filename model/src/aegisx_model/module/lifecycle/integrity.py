"""Hash the complete training contract, including labels and group ownership."""
import hashlib
import json


class Integrity:
    ARRAYS = ("features", "text", "event_text", "event_values", "coverage", "labels", "groups", "origins")

    @staticmethod
    def digest ( path ):
        with path.open("rb") as stream:
            return hashlib.file_digest(stream, "sha256").hexdigest()

    @staticmethod
    def seal ( directory ):
        path = directory / "manifest.json"
        manifest = json.loads(path.read_text())
        manifest["file_sha256"] = {name + ".npy": Integrity.digest(directory / (name + ".npy")) for name in Integrity.ARRAYS}
        manifest["fingerprint_version"] = 2
        manifest["fingerprint"] = hashlib.sha256(json.dumps(
            {"files": manifest["file_sha256"], "schema": manifest["input_schema"], "provenance": manifest["provenance"]},
            sort_keys=True, separators=(",", ":")).encode()).hexdigest()
        temporary = path.with_suffix(".tmp")
        temporary.write_text(json.dumps(manifest, indent=2)+"\n")
        temporary.replace(path)
        return manifest

    @staticmethod
    def verify ( directory, manifest ):
        if "fingerprint_version" not in manifest: return  # Archived v0.8 corpora.
        if manifest["fingerprint_version"] != 2: raise ValueError("Unsupported corpus fingerprint")
        expected = {name + ".npy" for name in Integrity.ARRAYS}
        if set(manifest.get("file_sha256", {})) != expected: raise ValueError("Incomplete corpus integrity manifest")
        for name, digest in manifest["file_sha256"].items():
            path = directory / name
            if path.is_symlink() or Integrity.digest(path) != digest: raise ValueError(f"Corpus changed: {name}")
        fingerprint = hashlib.sha256(json.dumps(
            {"files": manifest["file_sha256"], "schema": manifest["input_schema"], "provenance": manifest["provenance"]},
            sort_keys=True, separators=(",", ":")).encode()).hexdigest()
        if fingerprint != manifest["fingerprint"]: raise ValueError("Corpus contract changed")
