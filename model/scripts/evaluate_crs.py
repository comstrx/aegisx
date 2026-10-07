"""Evaluate positive CRS fixtures independently; no training or threshold tuning here."""
import argparse
import hashlib
import json
from pathlib import Path

import numpy as np
import onnxruntime as ort
import yaml

from aegisx_model.module.content import Content
from aegisx_model.module.features import Features
from aegisx_model.module.sources import Sources

parser=argparse.ArgumentParser()
parser.add_argument("--source",type=Path,default=Path("data/crs/tests/regression/tests"))
parser.add_argument("--artifact",type=Path,default=Path("weights"))
args=parser.parse_args()
metadata=json.loads((args.artifact/"metadata.json").read_text())
session_options=ort.SessionOptions()
session_options.intra_op_num_threads=1
runtime=ort.InferenceSession(str(args.artifact/"model.onnx"),sess_options=session_options,providers=["CPUExecutionProvider"])
content,schema=Content(),Features()
rows,groups,templates,seen=[],[],[],set()
skipped=0
for path in sorted(args.source.rglob("*.yaml")):
    if path.name[:3] not in {"930","931","932","933","934","941","942","944"}: continue
    document=yaml.safe_load(path.read_text())
    for case in document.get("tests",[]):
        for stage in case.get("stages",[]):
            request,output=stage.get("input",{}),stage.get("output",{})
            if not output.get("log",{}).get("expect_ids"):
                skipped+=1
                continue
            if any(name in request for name in ("raw_request","encoded_request")):
                skipped+=1
                continue
            uri=request.get("uri","/")
            body=request.get("data","")
            if not isinstance(uri,str) or not isinstance(body,str):
                skipped+=1
                continue
            if uri in {"/","/get","/post","/index.html"} and not body:
                skipped+=1
                continue
            payload=(uri+body).encode("utf-8")
            digest=hashlib.sha256(payload).hexdigest()
            if digest in seen: continue
            seen.add(digest)
            rows.append(content.row(payload[:16384],len(payload)))
            groups.append(path.name[:3])
            templates.append("t:"+hashlib.sha256(Sources.canonical(payload)).hexdigest())
values=schema.normalize(rows)
scores=np.array([runtime.run(["risk"],{"features":row.reshape(1,len(schema.names))})[0].item() for row in values])
report={"source":"OWASP CRS 4.29.0 positive regression fixtures","artifact_sha256":metadata["artifact_sha256"],
        "threshold":metadata["recommended_threshold"],"rows":len(rows),"skipped_stages":skipped,
        "notice":"External positive-only diagnostic, inspected during development; not a pristine acceptance set. Expected rule triggers are not production ground truth. No FPR estimate; fixtures were not used to train or calibrate.",
        "families":{name:{"rows":groups.count(name),"detected":int((scores[np.array(groups)==name]>=metadata["recommended_threshold"]).sum())} for name in sorted(set(groups))}}
report["detected"]=int((scores>=metadata["recommended_threshold"]).sum())
report["recall"]=report["detected"]/len(rows)
signatures=args.artifact/"source-signatures.json"
if signatures.exists():
    known=set(json.loads(signatures.read_text()))
    unseen=np.array([template not in known and "f:"+hashlib.sha256(row.astype("<f4").tobytes()).hexdigest() not in known
                     for template,row in zip(templates,values,strict=True)])
    count=int(unseen.sum())
    detected=int((scores[unseen]>=metadata["recommended_threshold"]).sum())
    report["source_disjoint"]={"rows":count,"detected":detected,"recall":detected/max(1,count),"excluded_overlap":int((~unseen).sum()),
        "method":"Exclude canonical templates or exact normalized vectors present anywhere in the training-source corpus; near-duplicates may remain"}
(args.artifact/"external-report.json").write_text(json.dumps(report,indent=2)+"\n")
print(json.dumps(report,indent=2))
