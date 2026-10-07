"""wrk/native-loopback measurements. Successful 200 throughput only; no capture/model."""
import argparse
import hashlib
import http.client
import json
import os
from pathlib import Path
import re
import signal
import socket
import statistics
import platform
import subprocess
import tempfile
import time

ROOT=Path(__file__).resolve().parents[2]
def port():
    with socket.socket() as sock:
        sock.bind(("127.0.0.1",0))
        return sock.getsockname()[1]
def ready(address,process):
    for _ in range(200):
        if process.poll() is not None: raise RuntimeError("Benchmark process stopped")
        try:
            connection=http.client.HTTPConnection("127.0.0.1",address,timeout=.2)
            connection.request("GET","/health")
            response=connection.getresponse()
            assert response.status==200 and response.read()==b"x"*128
            connection.close()
            return
        except (OSError,http.client.HTTPException): time.sleep(.025)
    raise RuntimeError("Benchmark listener not ready")
def stop(process):
    if process.poll() is None:
        os.killpg(process.pid,signal.SIGTERM)
        process.wait(timeout=30)
def affinity(command,cpus):
    return ["taskset","--cpu-list",cpus,*command] if cpus else command
def measure(address,options):
    result=subprocess.run(affinity(["wrk",f"-t{options.client_threads}",f"-c{options.connections}",f"-d{options.seconds}s","--latency",f"http://127.0.0.1:{address}/benchmark"],options.client_cpus),capture_output=True,text=True,check=True)
    raw=result.stdout+result.stderr
    if "Socket errors:" in raw or "Non-2xx or 3xx responses:" in raw: raise RuntimeError(raw)
    def value(pattern): return re.search(pattern,raw).group(1)
    return {"requests_per_second":float(value(r"Requests/sec:\s+([\d.]+)")),"requests":int(value(r"(\d+) requests in")),"raw":raw}
def main(options):
    options.output.parent.mkdir(parents=True,exist_ok=True)
    with tempfile.TemporaryDirectory(prefix="aegisx-native-") as temp:
        temp=Path(temp);fixture=temp/"backend"
        subprocess.run(["gcc","-O3","-Wall","-Wextra","-Werror",str(ROOT/"server/tests/fixtures/http_backend.c"),"-o",str(fixture)],check=True)
        upstream=port()
        backend=subprocess.Popen(affinity([str(fixture),str(upstream),str(options.backend_workers)],options.backend_cpus),start_new_session=True)
        report={"scope":"WSL Linux loopback; wrk + native epoll backend share host; HTTP/1.1 keepalive, fixed 128-byte body, no capture/model; every request forwarded; policy profile enables SQLite-backed decision cache and observed rolling rate context",
            "host":{"platform":platform.platform(),"logical_cpus":os.cpu_count(),"cpu_model":next((line.split(": ",1)[1] for line in Path("/proc/cpuinfo").read_text().splitlines() if line.startswith("model name")),"unknown")},"settings":vars(options).copy(),"artifacts":{},"trials":[]}
        report["settings"]["output"]=str(options.output)
        try:
            ready(upstream,backend)
            report["direct_backend"]=measure(upstream,options)
            for turn in range(options.trials):
                binaries=options.binary if turn%2==0 else list(reversed(options.binary))
                for name in binaries:
                    binary=Path(name).resolve()
                    report["artifacts"][str(binary)]=hashlib.sha256(binary.read_bytes()).hexdigest()
                    address=port();config=temp/"Aegisx.lua"
                    config.write_text(f'''set_listen("127.0.0.1:{address}")
set_upstream("127.0.0.1:{upstream}")
set_store(false)
set_model {{mode="off"}}
set_runtime {{threads={options.proxy_threads},max_in_flight=16384,write_buffer_bytes={options.write_buffer}}}
set_cache {{decisions=false,responses=false}}
set_limits {{rate_limit_10s=0}}
''')
                    if options.profile=="policy":
                        config.write_text(config.read_text()+f'''
set_store("{temp}/policy.db")
set_cache {{decisions=true,responses=false,decision_ttl_ms=60000}}
set_limits {{rate_limit_10s=1000000000}}
add_route {{name="bench",path="/",capture=false}}
''')
                    with (temp/"proxy.log").open("w+") as log:
                        process=subprocess.Popen(affinity([str(binary),"--config",str(config)],options.proxy_cpus),stdout=log,stderr=log,start_new_session=True)
                        try:
                            ready(address,process)
                            warm=argparse.Namespace(**vars(options));warm.seconds=2
                            measure(address,warm)
                            before=Path(f"/proc/{process.pid}/stat").read_text().split()
                            start=time.monotonic()
                            result={"binary":str(binary),"trial":turn+1,**measure(address,options)}
                            after=Path(f"/proc/{process.pid}/stat").read_text().split()
                            result["proxy_cpu_cores"]=(int(after[13])+int(after[14])-int(before[13])-int(before[14]))/os.sysconf("SC_CLK_TCK")/(time.monotonic()-start)
                            result["proxy_rss_bytes"]=int(after[23])*os.sysconf("SC_PAGE_SIZE")
                            print(json.dumps(result),flush=True);report["trials"].append(result)
                        except Exception:
                            log.seek(0);print(log.read());raise
                        finally:stop(process)
            report["summary"]={name:{"median_requests_per_second":statistics.median(item["requests_per_second"] for item in report["trials"] if item["binary"]==name)} for name in report["artifacts"]}
            options.output.write_text(json.dumps(report,indent=2)+"\n")
        finally:stop(backend)
if __name__=="__main__":
    parser=argparse.ArgumentParser()
    parser.add_argument("--binary",action="append",required=True)
    parser.add_argument("--output",type=Path,required=True)
    parser.add_argument("--trials",type=int,default=3)
    parser.add_argument("--seconds",type=int,default=15)
    parser.add_argument("--connections",type=int,default=128)
    parser.add_argument("--client-threads",type=int,default=2)
    parser.add_argument("--backend-workers",type=int,default=2)
    parser.add_argument("--proxy-threads",type=int,default=2)
    parser.add_argument("--write-buffer",type=int,default=0)
    parser.add_argument("--profile",choices=["forwarding","policy"],default="forwarding")
    parser.add_argument("--proxy-cpus")
    parser.add_argument("--backend-cpus")
    parser.add_argument("--client-cpus")
    main(parser.parse_args())
