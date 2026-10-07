"""Local cache-aside and background coverage measurements; not a capacity claim."""
import asyncio
import argparse
import hashlib
import json
import os
from pathlib import Path
import signal
import statistics
import subprocess
import tempfile
import time
from collections import Counter
from benchmark import backend,port

TOKEN="aegisx-local-benchmark-only-0000000000"
BINARY=Path(os.environ.get("AEGISX_BIN",Path(__file__).parents[2]/"dist/aegisx")).resolve()
async def exchange(address,path="/",actor=None,admin=False):
    reader,writer=await asyncio.open_connection("127.0.0.1",address)
    headers=f"GET {path} HTTP/1.1\r\nHost: 127.0.0.1:{address}\r\nConnection: close\r\n"
    if actor is not None:headers+=f"x-test-actor: {actor}\r\n"
    if admin:headers+="Authorization: Bearer "+TOKEN+"\r\n"
    start=time.perf_counter()
    writer.write((headers+"\r\n").encode())
    await writer.drain()
    header=await reader.readuntil(b"\r\n\r\n")
    status=int(header.split(b" ",2)[1])
    if status not in ({200} if admin else {200,503}):raise RuntimeError(header.decode())
    length=int(next(line.split(b":",1)[1] for line in header.lower().split(b"\r\n") if line.startswith(b"content-length:")))
    body=await reader.readexactly(length)
    elapsed=(time.perf_counter()-start)*1000
    writer.close()
    await writer.wait_closed()
    return body,elapsed,status

async def trial(upstream,mode,waiting):
    address,admin=port(),port()
    with tempfile.TemporaryDirectory(prefix="aegisx-pipeline-") as tmp:
        config=Path(tmp)/"config.lua"
        config.write_text(f'''
set_listen("127.0.0.1:{address}")
set_queue {{capacity={256 if waiting else 0},timeout_ms=1000}}
set_upstream("127.0.0.1:{upstream}")
set_store("{tmp}/events.db")
set_control {{enabled=true,listen="127.0.0.1:{admin}"}}
set_identity {{actor_header="x-test-actor",trusted_peers={{"127.0.0.1/32"}}}}
set_cache {{decisions={str(mode!="background").lower()},decision_ttl_ms=60000}}
set_model {{mode="{"observe" if mode=="background" else "off"}",queue_capacity=64}}
set_telemetry {{capture="summary",recent_capacity=16}}
set_limits {{queue_capacity=65536}}
add_route {{name="bench",path="/",capture={str(mode=="background").lower()}}}
''')
        env=os.environ.copy()
        env["AEGISX_ADMIN_TOKEN"]=TOKEN
        with (Path(tmp)/"log").open("w+") as log:
            process=subprocess.Popen([str(BINARY),"--config",str(config)],env=env,stdout=log,stderr=log)
            try:
                for _ in range(200):
                    if process.poll() is not None:log.seek(0);raise RuntimeError(log.read())
                    try:
                        await exchange(admin,"/api/v1/state",admin=True)
                        _,connection=await asyncio.open_connection("127.0.0.1",address)
                        connection.close()
                        await connection.wait_closed()
                        break
                    except OSError:await asyncio.sleep(.025)
                elapsed=[]
                accepted=[]
                statuses=Counter()
                async def client(index):
                    for step in range(375):
                        actor=f"actor-{index}-{step}" if mode=="cold" else "shared-actor"
                        _,latency,status=await exchange(address,actor=actor)
                        statuses[status]+=1
                        elapsed.append(latency)
                        if status==200:accepted.append(latency)
                start=time.perf_counter()
                await asyncio.gather(*(client(index) for index in range(8)))
                duration=time.perf_counter()-start
                for _ in range(200):
                    data,_,_=await exchange(admin,"/api/v1/state",admin=True)
                    state=json.loads(data)
                    if not state["analysis"] or (state["analysis"]["finished"]==state["analysis"]["submitted"] and state["storage"]["committed_batches"]+state["storage"]["pressure_rejections"]==len(elapsed)):break
                    await asyncio.sleep(.025)
                ordered=sorted(elapsed)
                return {"mode":mode,"waiting_enabled":waiting,"queue":state["queue"],"requests":len(elapsed),"attempts_per_second":len(elapsed)/duration,"forwarded_requests_per_second":len(accepted)/duration,"statuses":dict(statuses),
                        "p50_ms":statistics.median(elapsed),"p95_ms":ordered[int(len(ordered)*.95)],
                        "decisions":state["decisions"],"analysis":state["analysis"],"dropped_events":state["dropped_events"],"storage":state["storage"]}
            finally:
                process.send_signal(signal.SIGTERM)
                await asyncio.to_thread(process.wait,15)

async def main(output,waiting,modes):
    server=await asyncio.start_server(backend,"127.0.0.1",0)
    report={"scope":"WSL loopback; 8 clients, 3000 requests each trial, fresh downstream connections, shared Python client/backend host. Cold uses distinct trusted test actors. Model disabled in cache trials. Background captures summary with a 64-job queue.","binary_sha256":hashlib.sha256(BINARY.read_bytes()).hexdigest(),"trials":[]}
    try:
        for mode in modes:
            for _ in range(3):
                result=await trial(server.sockets[0].getsockname()[1],mode,waiting)
                print(json.dumps(result),flush=True)
                report["trials"].append(result)
    finally:server.close();await server.wait_closed()
    output.write_text(json.dumps(report,indent=2)+"\n")

if __name__=="__main__":
    parser=argparse.ArgumentParser()
    parser.add_argument("--output",type=Path,required=True)
    parser.add_argument("--waiting",action="store_true")
    parser.add_argument("--modes",nargs="+",choices=["warm","cold","background"],default=["warm","cold","background"])
    args=parser.parse_args()
    asyncio.run(main(args.output,args.waiting,args.modes))
