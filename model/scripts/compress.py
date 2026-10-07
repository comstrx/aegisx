import json
from pathlib import Path
from aegisx_model.module.sources import Sources
from aegisx_model.module.compress import Compression
data=Sources.http_params(Path("data/sources/payload_full.csv"),Path("data/fuzzdb"),Path("data/wcp"),augment=True,security=Path("data/seclists"))
print(json.dumps(Compression.run(data,Path("weights")),indent=2))
