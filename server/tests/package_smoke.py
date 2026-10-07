"""Verify relocation of the distribution executable without asset/model directories."""

import http.client
import json
from pathlib import Path
import re
import shutil
import signal
import socket
import subprocess
import tempfile
import tomllib
import unittest

from integration_support import TOKEN
from lifecycle import BINARY, free_port, until


class PackageTests ( unittest.TestCase ):

    def test_relocated_binary_serves_embedded_model_panel_and_api ( self ):

        with tempfile.TemporaryDirectory(prefix="aegisx-package-") as directory:
            root = Path(directory)
            binary = root / "aegisx"
            shutil.copy2(BINARY, binary)
            environment = {"PATH":"/usr/bin:/bin", "AEGISX_ADMIN_TOKEN":TOKEN}
            fixtures = json.loads((Path(__file__).parents[2] / "model/weights/parity.json").read_text())
            fixture = max(fixtures, key=lambda item: item["score"])
            tensors = root / "input.json"
            tensors.write_text(json.dumps(fixture))
            score = json.loads(subprocess.check_output([str(binary), "score", "--input", str(tensors)], cwd=root, env=environment))
            self.assertAlmostEqual(score["risk"], fixture["score"], places=5)
            tensors.unlink()
            proxy_port, admin_port, upstream_port = free_port(), free_port(), free_port()
            config = root / "Aegisx.lua"
            config.write_text(f'''set_listen("127.0.0.1:{proxy_port}")
set_upstream("127.0.0.1:{upstream_port}")
set_model("off")
set_store(false)
set_control {{ enabled=true, listen="127.0.0.1:{admin_port}" }}
''')
            def fetch ( path, token=None ):
                try:
                    connection = http.client.HTTPConnection("127.0.0.1", admin_port, timeout=1)
                    connection.request("GET", path, headers={"Authorization":f"Bearer {token}"} if token else {})
                    response = connection.getresponse()
                    result = response.status, response.read()
                    connection.close()
                    return result
                except OSError: return None
            with (root / "process.log").open("w+") as log:
                process = subprocess.Popen([str(binary),"--config",str(config)], cwd=root, env=environment, stdout=log, stderr=log)
                try:
                    def ready ():
                        try:
                            with socket.create_connection(("127.0.0.1", proxy_port), timeout=0.1): return True
                        except OSError: return False
                    until(ready)
                    home = until(lambda: fetch("/"))
                    self.assertEqual(home[0],200)
                    self.assertIn(b"AegisX",home[1])
                    asset = re.search(rb'<script[^>]*src="([^"]+)"',home[1])[1].decode()
                    self.assertEqual(fetch(asset)[0],200)
                    self.assertEqual(fetch("/api/v1/state")[0],401)
                    status, state = fetch("/api/v1/state",TOKEN)
                    self.assertEqual(status,200)
                    self.assertEqual(json.loads(state)["version"],tomllib.loads((Path(__file__).parents[1] / "Cargo.toml").read_text())["package"]["version"])
                    self.assertEqual({path.name for path in root.iterdir()}, {"aegisx","Aegisx.lua","process.log"})
                finally:
                    process.send_signal(signal.SIGTERM)
                    process.wait(timeout=12)
                self.assertEqual(process.returncode,0)


if __name__ == "__main__": unittest.main(verbosity=2)
