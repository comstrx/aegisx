import hashlib
import json
import numpy as np
import pytest
from aegisx_model.module.sources import Sources
from aegisx_model.module.wcp import Wcp

def capture(directory, name, split, requests):
    raw=json.dumps(requests).encode()
    (directory/name).write_bytes(raw)
    return {"name":name,"split":split,"sha256":hashlib.sha256(raw).hexdigest()}

def test_wcp_excludes_external_headers_and_host_and_verifies_digest(tmp_path):
    first=capture(tmp_path,"train.json","train",[{"url":"https://user:secret@example.test/item?q=1#token","data":"body","headers":{"Authorization":"secret"}}])
    second=capture(tmp_path,"external.json","external",[{"url":"https://test.invalid/heldout"}])
    (tmp_path/"manifest.json").write_text(json.dumps({"members":[first,second]}))
    examples,_=Wcp.examples(tmp_path)
    assert [row[0] for row in examples]==[b"/item?q=1body"]
    assert Wcp.examples(tmp_path,"external")[0][0][0]==b"/heldout"
    (tmp_path/"train.json").write_text("[]")
    with pytest.raises(ValueError,match="digest"): Wcp.examples(tmp_path)

def test_augmentation_preserves_groups_and_does_not_invent_runtime_context(tmp_path):
    path=tmp_path/"samples.csv"
    path.write_text("payload,label\nhello world,norm\nselect * from data,anom\n")
    data=Sources.http_params(path,augment=True)
    assert len(data.values)==6
    assert len(set(data.groups[:3]))==1 and len(set(data.groups[3:]))==1
    assert data.groups[0]!=data.groups[3]
    assert not np.any(data.values[:,:16]) and not np.any(data.values[:,32:40])
    assert np.any(data.values[:,40:])
    assert data.provenance["augmentation"]!="none"
