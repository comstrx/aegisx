# Local performance evidence · 2026-10-02

Current release: [v0.10 transport verification, Nginx comparison and 395M research result](v010.md).

Previous release: [v0.9 verification, compact inference and durable-decision integrity](v09.md).

New comparison: [installed Nginx versus AegisX and the direct backend](nginx-comparison.md), using matched local profiles.

Historical reports: [v0.8 text/journey integration](v08.md), [v0.7 bounded waiting](v07.md), [v0.6](v06.md), [v0.5](v05.md), [v0.4](v04.md), and [v0.3](v03.md). Each report describes its own artifact and measurement scope; older numbers are not current capacity claims. The remaining sections preserve v0.1→v0.2 evidence.

## Historical v0.2 comparison

Results compare the preserved v0.1 release with v0.2. Full binary hashes, individual trials and latencies are in [the long run](2026-10-01-v02.json); [the initial short run](2026-10-01-v02-smoke.json) is retained rather than cherry-picked away.

## Method

Same WSL machine; Rust release executables, two proxy worker threads, one local Python asyncio backend and client, HTTP/1.1 persistent connections, 128-byte bodies, 16 concurrent clients. Model off; capture off/on tested separately. Three trials per binary/mode, alternating order; 512 warmup requests plus 60,000 measured requests per trial. Storage queue is 65,536, retention 1,000,000. No build or training job ran during measurement. The load generator and backend share the host, so this is not a proxy capacity certification.

Reproduce from server/ with the preserved or separately built baseline executable:

```sh
python3 tests/benchmark.py --baseline ../tmp/baseline/aegisx-v0.1 --current target/release/aegisx --requests 60000 --output benchmarks/local.json
```

## Results

Median across three trials:

| Mode | v0.1 requests/s | v0.2 requests/s | v0.1 p95 | v0.2 p95 |
| --- | ---: | ---: | ---: | ---: |
| Proxy only | 30,146 | 30,753 | 0.819 ms | 0.758 ms |
| Full capture, saturated | 19,101 | 18,140 | 1.416 ms | 1.496 ms |

The approximately +2% proxy-only change is within the observed run variability; **no meaningful speedup is established**. The shorter 6,000-request run showed +0.8%. New routing, health and policy machinery preserved roughly comparable forwarding performance in this setup.

Captured mode is not equivalent work: v0.2 additionally builds admission history/features with model off and attaches configuration/route metadata. Its longer-run median forwarding throughput fell approximately 5%; the short-run difference was approximately 11%. These are observed costs, not evidence that added observability is free.

**Both versions overflowed their telemetry queues in the longer captured run.** Per-trial dropped event counts:

| Version | Trial 1 | Trial 2 | Trial 3 |
| --- | ---: | ---: | ---: |
| v0.1 | 326,796 | 321,045 | 321,230 |
| v0.2 | 245,597 | 244,831 | 254,441 |

Drops include the warmup. Counts are events, not necessarily unique requests. Neither captured-mode throughput number represents complete persisted request journeys. The writer change reduced median dropped events by about 24%, but did not eliminate overload. All short-run trials reported zero dropped events, partly because the large queue could absorb the short burst and drain afterward.

## Implemented performance work and remaining constraint

- Lua executes only on startup/reload, with immutable per-request snapshots.
- Disabled capture skips lifecycle JSON construction.
- A single-backend pool avoids candidate-vector allocation and its selection mutex.
- SQLite uses cached inserts and transactions of up to 64 immediately available events.
- Inference/health checks/storage are bounded and separated from forwarding.
- Runtime controls expose worker count, request/backend concurrency, write buffering and connection reuse.
- Full capture remains limited by writer/serialization/index throughput. Reducing capture by route is available now; v0.3 subsequently added summary/sampling controls; durable lossless capture and broader storage optimizations remain future work.
- This benchmark does not measure TLS, model latency, multi-host networking, long-lived streams, attack resistance or enterprise readiness.
