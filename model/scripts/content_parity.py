import json
from pathlib import Path
from aegisx_model.module.content import Content
cases = [b"", b"/products?q=hello+world", b"/login?id=1%2520OR%25201=1", bytes(range(256)), b"&&&&%zz%00;..<script>", b"a" * 16384, b"%2B%252B%252f"]
values = [{"sample": list(sample), "total": len(sample) + (10 if index == 5 else 0), "expected": Content().extract(sample, len(sample) + (10 if index == 5 else 0))} for index, sample in enumerate(cases)]
Path("weights/content-parity.json").write_text(json.dumps(values) + "\n")
