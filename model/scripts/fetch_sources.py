"""Fetch pinned public research inputs, verify hashes, never execute their content."""
import hashlib
import io
import json
from pathlib import Path
import tarfile
import urllib.request

ROOT=Path(__file__).resolve().parents[1]
MANIFEST=ROOT/"sources.json"
def download(url,expected):
    request=urllib.request.Request(url,headers={"User-Agent":"AegisX-research-fetch"})
    with urllib.request.urlopen(request,timeout=90) as response:
        content=response.read(64*1024*1024+1)
    if len(content)>64*1024*1024: raise ValueError("Source archive exceeds budget")
    actual=hashlib.sha256(content).hexdigest()
    if expected and expected!=actual: raise ValueError("Source digest changed: "+url)
    return content

def main():
    spec=json.loads(MANIFEST.read_text())
    for name,entry in spec["http_params"]["files"].items():
        target=ROOT/"data/sources"/name
        if target.exists() and hashlib.sha256(target.read_bytes()).hexdigest()==entry["sha256"]: continue
        data=download(entry["url"],entry["sha256"])
        target.parent.mkdir(parents=True,exist_ok=True)
        target.write_bytes(data)
    for kind in ("fuzzdb", "seclists"):
        for name,entry in spec[kind]["files"].items():
            if Path(name).is_absolute() or ".." in Path(name).parts: raise ValueError("Unsafe source path")
            target=ROOT/"data"/kind/name
            target.parent.mkdir(parents=True,exist_ok=True)
            if not target.exists() or hashlib.sha256(target.read_bytes()).hexdigest()!=entry["sha256"]:
                target.write_bytes(download(entry["url"],entry["sha256"]))
        training={name:item["sha256"] for name,item in spec[kind]["files"].items() if name.endswith(".txt") and not name.startswith("_") and "Generic_SQLI" not in name}
        (ROOT/"data"/kind/"manifest.json").write_text(json.dumps({"files":training},indent=2)+"\n")
    archive=download(spec["crs"]["url"],spec["crs"]["sha256"])
    with tarfile.open(fileobj=io.BytesIO(archive),mode="r:gz") as bundle:
        for member in bundle:
            if not member.isfile(): continue
            relative=Path(*Path(member.name).parts[1:])
            if relative.is_absolute() or ".." in relative.parts: raise ValueError("Unsafe archive path")
            if not (str(relative).startswith("tests/regression/tests/") or relative.name in {"LICENSE","NOTICE"}): continue
            if member.size>2*1024*1024: raise ValueError("Fixture exceeds budget")
            target=ROOT/"data/crs"/relative
            target.parent.mkdir(parents=True,exist_ok=True)
            stream=bundle.extractfile(member)
            if stream is not None: target.write_bytes(stream.read())
    print("Pinned sources verified. Training and evaluation remain separate.")

if __name__=="__main__": main()
