"""Read pinned, bounded ZIP members via HTTP ranges; never replay public requests."""
import hashlib
import json
from pathlib import Path
import struct
import urllib.request
import zlib

ROOT=Path(__file__).resolve().parents[1]
def fetch(entry,spec):
    target=ROOT/"data/wcp"/Path(entry["name"]).name
    if target.exists() and hashlib.sha256(target.read_bytes()).hexdigest()==entry["sha256"]:return
    def part(start,length):
        if length>2*1024*1024:raise ValueError("Compressed member exceeds budget")
        request=urllib.request.Request(spec["url"],headers={"Range":f"bytes={start}-{start+length-1}","If-Match":spec["etag"],"User-Agent":"AegisX-research"})
        with urllib.request.urlopen(request,timeout=60) as response:
            if response.status!=206 or response.headers.get("ETag")!=spec["etag"]:raise ValueError("Archive version/range changed")
            raw=response.read(length+1)
            if len(raw)!=length:raise ValueError("Invalid range length")
            return raw
    header=part(entry["offset"],30)
    if header[:4]!=b"PK\x03\x04":raise ValueError("Invalid local ZIP header")
    name,extra=struct.unpack_from("<HH",header,26)
    packed=part(entry["offset"]+30+name+extra,entry["compressed"])
    decoder=zlib.decompressobj(-15)
    raw=decoder.decompress(packed,entry["bytes"]+1)
    if not decoder.eof or len(raw)!=entry["bytes"] or zlib.crc32(raw)!=entry["crc"]:raise ValueError("Invalid ZIP member")
    if hashlib.sha256(raw).hexdigest()!=entry["sha256"]:raise ValueError("Member digest changed")
    target.parent.mkdir(parents=True,exist_ok=True)
    target.write_bytes(raw)
def main():
    spec=json.loads((ROOT/"sources.json").read_text())["wcp_benign"]
    for entry in spec["members"]:fetch(entry,spec)
    url = f"https://raw.githubusercontent.com/openappsec/waf-comparison-project/{spec['commit']}/LICENSE"
    target = ROOT / "data/wcp/LICENSE"
    if not target.exists() or hashlib.sha256(target.read_bytes()).hexdigest() != spec["license_sha256"]:
        with urllib.request.urlopen(url, timeout=30) as response: raw = response.read(65537)
        if len(raw) > 65536 or hashlib.sha256(raw).hexdigest() != spec["license_sha256"]: raise ValueError("License digest mismatch")
        target.write_bytes(raw)
    (ROOT/"data/wcp/manifest.json").write_text(json.dumps(spec,indent=2)+"\n")
    print(f"Verified {len(spec['members'])} pinned benign captures; training and external captures separate.")
if __name__=="__main__":main()
