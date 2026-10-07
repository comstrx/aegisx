import csv
import json
from pathlib import Path
import numpy as np
from aegisx_model.module.content import Content
from aegisx_model.module.sources import Sources

def test_content_extraction_matches_shared_byte_cases():
    path=Path(__file__).parents[1]/"weights/content-parity.json"
    for case in json.loads(path.read_text()):
        assert np.allclose(Content().extract(bytes(case["sample"]),case["total"]),case["expected"],atol=1e-6)

def test_source_keeps_templates_and_identical_inputs_in_one_group(tmp_path):
    path=tmp_path/"source.csv"
    with path.open("w",newline="") as stream:
        writer=csv.writer(stream)
        writer.writerow(["payload","label"])
        writer.writerows([["id=123","norm"],["id=456","norm"],["id=123","norm"],["';DROP TABLE users--","anom"]])
    data=Sources.http_params(path)
    assert len(data.values)==3
    assert data.groups[0]==data.groups[1]
    assert not data.values[:,:16].any()
    assert not data.values[:,32:40].any()
    assert data.source=="http_params"


def test_security_sources_verify_integrity_and_group_augmentations(tmp_path):
    import hashlib
    import pytest
    source = tmp_path / "source.csv"
    source.write_text("payload,label\nhello world,norm\n")
    directory = tmp_path / "security"
    directory.mkdir()
    attack = directory / "LDAP.txt"
    attack.write_bytes(b"# ignored comment\n*)(uid=*))(|(uid=*\n")
    manifest = directory / "manifest.json"
    manifest.write_text(json.dumps({"files": {"LDAP.txt": hashlib.sha256(attack.read_bytes()).hexdigest()}}))
    data = Sources.http_params(source, security=directory, augment=True)
    assert data.source == "http_corpus"
    assert len(data.values) == 6
    assert len(set(data.groups[:3])) == 1
    assert len(set(data.groups[3:])) == 1
    assert not set(data.groups[:3]) & set(data.groups[3:])
    assert set(data.origins[3:]) == {"seclists_ldap", "seclists_ldap_query", "seclists_ldap_json"}
    attack.write_bytes(b"modified payload")
    with pytest.raises(ValueError, match="digest mismatch"):
        Sources.http_params(source, security=directory)
    manifest.write_text(json.dumps({"files": {"../source.csv": "invalid"}}))
    with pytest.raises(ValueError, match="Unsafe source path"):
        Sources.http_params(source, security=directory)
