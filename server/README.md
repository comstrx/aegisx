# AegisX server · 0.10

A local Linux reverse proxy with configurable routing, load balancing, TLS, behavioral context and optional ONNX inference. One Rust binary crate; no mandatory external database, broker or inference service.

Version 0.9 adds coalesced SQLite decision lookups, verified durable records, shared immutable cached verdicts, constant-time rolling totals within each second, faster bounded normalization and independent content/journey thresholds. The embedded 895,178-parameter student was selected after three experiments including a 125.64M transfer teacher. Inference remains post-completion. [Configuration](configuration.md), [backend contracts](contracts/README.md), [panel](../panel/README.md) and [measured evidence](benchmarks/v09.md).

Version 0.10 adds an atomic global admission limit, avoids unneeded upstream counter updates, and tests buffered HTTP streaming at both integration and dependency levels. The application remains one Rust binary crate; two narrowly patched Pingora dependencies share its Cargo workspace and lockfile. Background inference and committed-decision enforcement are unchanged.

Measured forwarding medians improve 2.7–5.7% in this local run, while the policy-profile median regresses 2.5%. Memory does not consistently fall, and throughput remains below half of the installed Nginx build. See [0.10 verification and all trials](benchmarks/v010.md).

## Run and reload

```sh
# From repository root: python3 scripts/build.py
# Then from server/:
./target/release/aegisx --config Aegisx.lua --check
./target/release/aegisx --config Aegisx.lua
# From another terminal, using the actual process ID:
kill -HUP PID
```

The supplied configuration targets your existing backend on port 3000. The process stays in the foreground for a service manager. SIGTERM drains active work; SIGINT stops quickly. Shutdown has a one-second grace period followed by up to five seconds of runtime shutdown. It cannot guarantee completion of longer backend operations.

SIGHUP parses and validates a fresh Lua configuration, then atomically publishes it. Each request retains its original configuration snapshot. Invalid reloads keep the previous configuration. Unchanged pools retain health/load history. Listener/TLS files, worker resources, storage destination/retention, and model artifact changes require restart. Changing the content of a certificate/model file at the same path also requires restart. Enabling inference after starting with every model mode off requires restart.

`--check` validates Lua, referenced pools, TLS material and enabled model artifacts without starting a listener or modifying the database. It does not test backend reachability.

## Lua DSL

```lua
set_listen("127.0.0.1:8080")
add_upstream("api", { address="127.0.0.1:3000", weight=2, max_in_flight=200 })
add_upstream("api", { address="127.0.0.1:3001", weight=1, max_in_flight=100 })
set_default_upstream("api")
set_balancer("api", {
    policy="least_conn", max_fails=2, cooldown_ms=5000,
    health_interval_ms=1000, health_timeout_ms=300, connect_attempts=2
})
set_model { mode="observe", queue_capacity=64, scan_bytes=16384, max_queue_age_ms=5000, threshold=0.95 }
set_store("aegisx.sqlite3")
set_limits { rate_limit_10s=500, max_body_bytes=1048576 }
set_runtime { threads=2, max_in_flight=4096, write_buffer_bytes=0 }
set_queue { capacity=256, timeout_ms=1000 }
set_headers("response", { ["x-proxy"]="aegisx" })
add_route {
    name="orders", host="app.example.com", path="/api/orders", methods={"POST"},
    max_body_bytes=65536, rate_limit_10s=20, model="observe"
}
add_route { name="private", path="/internal", deny=true }
add_route { name="health", path="/health", exact=true, model="off", capture=false }
```

For one backend, `set_upstream("127.0.0.1:3000")` defines the implicit `default` pool. `set_balancer("default", {...})` can configure it. `add_upstream` accepts either an address string or a settings table. If a pool has explicitly added backends, `set_upstream` does not append another.

Routes inherit the default pool when `upstream` is omitted. Set `set_default_upstream(false)` to return 404 for unmatched requests; then non-deny routes must name a pool. Matches combine exact/wildcard host, path, methods and match_headers. Priority is: exact host, wildcard suffix, hostless; then longest host/path, exact path, method specificity, header count and route name. Prefixes respect segment boundaries: `/api` matches `/api/x`, not `/apix`.

Additional route fields: `upstream`, `strip_prefix`, `timeout_ms`, `threshold`, `preserve_host`, `request_headers`, `response_headers`. Prefix stripping preserves the query string. Encoded ASCII unreserved characters are normalized before routing and forwarding. Ambiguous paths (dot segments, duplicate slashes, backslashes, semicolons, malformed/double escapes and encoded separators) are rejected, not passed through for a backend to reinterpret. Applications requiring these paths need a future explicit compatibility policy.

Per-route values override inherited values, except that **global actor rate limits remain an additional hard cap**. Route rate limits apply separately to (route name, actor identity). Route `rate_limit_10s=0` disables only that route's additional cap. Each limit uses ten one-second buckets, approximately 9–10 seconds. Blocked attempts count after reaching the corresponding admission stage. By default NAT clients share the direct-peer IP budget; explicitly trusted actor headers can provide another identity.

Table setters replace their setting group using defaults for omitted fields; repeated calls do not merge older tables. Header names are case-insensitive and route headers override global headers. Framing, connection, forwarding and request-ID headers cannot be configured as custom headers. Unknown options, invalid bounds and duplicate matches fail validation.

Only DSL functions are exposed to Lua: no OS/filesystem/import functions. Source is limited to 64 KiB, Lua memory to 1 MiB, and execution to 10,000 VM instructions. Lua runs at startup/reload, never per request. Relative database, model and certificate paths resolve against the Lua file.

## Balancing and transport

| Policy | Decision |
| --- | --- |
| `round_robin` | Weighted rotation over available backends |
| `least_conn` | Lowest (active requests + 1) / weight, rotating ties |
| `adaptive` | Lowest EWMA time-to-response-headers × (active requests + 1) / weight |
| `first` | First available configured backend, allowing ordered failover |

All policies respect per-backend concurrency and health. Adaptive balancing uses measured transport behavior, **not an AI routing model or backend-internal telemetry**. Its initial latency estimate is 50 ms; EWMA retains 7/8 of the previous value. Slow clients do not inflate the response-body transfer time into this estimate. This policy has no separate exploration scheduler.

Passive checks count consecutive connection/transport failures and HTTP 5xx responses, temporarily excluding a backend. Active checks default to optional TCP liveness probes. Configuring health_path selects HTTP/HTTPS readiness probes with the backend TLS verification policy and expected health_status. They run concurrently off the request path, with at most 64 configured backends. No available capacity returns 503.

`connect_attempts` is the total attempt limit, 1–3 (default 1). Retries select distinct backends and occur only when establishing a connection fails. Errors after connection establishment are not replayed, including POST errors. A reused stale connection can therefore yield 502 rather than a replay. The per-attempt timeout is not an overall deadline.

HTTP/1.1 bodies stream with Pingora backpressure. Default downstream write buffering is 4 KiB. Each already-ready response batch flushes immediately, including headers and non-final chunks; AegisX does not wait for a later chunk to fill this buffer. Lua `write_buffer_bytes=0` disables buffering; the upper bound remains 64 KiB. Upstream connections are pooled. `pool_idle_seconds` and `keepalive_seconds` default to 30. Downstream keepalive 0 disables reuse; upstream pool idle must be 1–600 seconds. Declared oversized bodies fail before forwarding; an oversized streamed body may already have delivered earlier bytes upstream.

### TLS

```lua
set_tls { cert="certificates/fullchain.pem", key="certificates/key.pem" }
add_upstream("secure", {
    address="127.0.0.1:8443", tls=true, server_name="backend.example.com",
    ca_file="certificates/backend-ca.pem"
})
set_default_upstream("secure")
```

TLS upstreams require a verification name; certificates and hostnames are verified. Omit `ca_file` for system trust. There is no disable-verification option. Certificates are supplied by the operator; ACME/automatic renewal is not implemented. The supported/tested transport surface is HTTP/1.1 over HTTP or HTTPS.

Host defaults to the selected upstream's IP:port. `set_preserve_host(true)` or a route override preserves the original Host. TLS SNI is independently configured by `server_name`. Forwarded/X-Forwarded-* input is stripped; forwarded IP is the direct network peer, and scheme reflects the listener. There is no trusted proxy-chain resolver yet.

## Inference, observation and limits

| Mode | Behavior |
| --- | --- |
| `off` | No model worker required for this route |
| `observe` | Queue after completion; record results without model-generated restrictions |
| `background` | Queue after completion; optional future restrictions and cooperative cancellation |
| `enforce` | Rejected: inline enforcement was removed |

The transport boundary is fixed: admission applies explicit rules and existing decisions; once forwarding starts, late AI results never interrupt it. Network errors, I/O timeouts and body limits remain normal transport protections. Cancellation requires an explicitly cooperating backend and never guarantees rollback.

The embedded distilled classifier has 895,178 trained parameters, 296 numeric features and byte-journey-v1 tensors. The UINT8 artifact is 1.22 MB. The 125.64M transfer teacher runs only during development; its pretrained encoder was frozen. Internal content recall improves, but external CRS coverage remains low and false-positive/recall tradeoffs remain. Automatic restrictions require explicit allow_unvalidated=true for experiments or an independently evaluated artifact with operator-asserted deployment_ready=true. The trainer always writes false. See [model evidence](../model/README.md).

External artifacts use `set_model {mode="observe", directory="../model/runs/experiment"}`. Startup validates SHA-256, schema, architecture metadata and actual inference. External ONNX is capped at 64 MiB; metadata at 64 KiB. A hash verifies consistency, not authorship.

One CPU worker processes a bounded queue (1–4096, default 64); jobs older than `max_queue_age_ms` (1–60,000, default 5,000) expire before execution. URI and body-prefix sampling is bounded by `scan_bytes` (0–65,536, default 16,384). After forwarding terminates, an envelope contains admission counters, outcome/byte/timing/attempt metadata, a request correlation ID and bounded backend reports. Lifecycle phases are retained alongside the analysis result, without duplicating the entire event list inside that result. The worker normalizes 296 features, encodes bounded text/event tensors, runs ONNX and validates content/journey/combined scores. Authorized future restrictions commit to SQLite before coherent cache updates. Raw text is discarded after inference, not persisted. This is not a full-body malware scanner or unrestricted log reader.

Requests denied before forwarding and public-response-cache hits do not enqueue model analysis; explicit admission/rate controls still apply. Capture is independent of inference. Default `on_overload="reject"` reserves an analysis slot before forwarding; saturation returns 503 without touching the backend. `on_overload="skip"` opts into completion-time best-effort submission and records analysis_skipped. Queue statistics distinguish admission rejections, skipped, expired and failed work. Captured jobs are journaled as normalized numbers before inference when SQLite is enabled; committed pending legacy numeric jobs recover in observation-only mode when compatible. Text-model jobs become recovery_unavailable because volatile text cannot be reconstructed. Active requests, raw queue samples and jobs not yet committed remain volatile. Results from old configuration snapshots or before a later operator decision are not allowed to overwrite current operator intent.

Captured requests build bounded numeric peer history even with the model off, supporting later labeled-data export. History contains observed counts, gaps, concurrency and latency; same-second rolling totals require no ten-bucket scan, timestamps cannot rewind the window after lock contention, and older completion updates cannot overwrite newer latency; model scores and model-induced rejection/error counts do not feed future features. Context is partitioned into configurable hash shards. Idle TTL defaults to 120 seconds; inactive entries expire on access or capacity cleanup. Active entries cannot be evicted. Each global/route map is independently bounded; an exhausted shard yields 503. See set_context in configuration.md.

For pure forwarding: `set_model("off")`, `set_store(false)`, disabled control/webhooks/caches, and no enabled rate limits. This starts no inference/storage workers, performs no history updates, and skips lifecycle JSON construction.

| Group | Defaults and bounds |
| --- | --- |
| limits | timeout 10,000 ms (100–120,000); body 1 MiB (1–16 MiB); sources 10,000 (1–100,000); storage queue 1,024 (16–65,536); retention 100,000 (100–1,000,000); global rate 0/off |
| runtime | threads 2 (1–64); active requests 4,096 (1–100,000); write buffer 0 (0–65,536); keepalive 30 s (0–600); pool idle 30 s (1–600) |
| pools | at most 32 pools, 64 total backends; weight 1 (1–1000); backend active cap 0/unlimited (0–100,000) |
| health | max_fails 2 (1–10); cooldown 5,000 ms (100–600,000); active interval 0/off or 100–60,000 ms; probe timeout 300 ms (50–1000) |
| routes | at most 1024; 32 custom headers per scope; header values at most 4 KiB |

These bound post-header request work; they are not a complete connection-flood defense. I/O timeouts are not total request deadlines.

## Lifecycle and persistence

```sh
./target/release/aegisx inspect --database aegisx.sqlite3 --limit 30
./target/release/aegisx inspect --database aegisx.sqlite3 --request REQUEST_ID
```

By default AegisX generates, forwards and echoes X-Request-ID. Header names, propagation, trusted UUID preservation and actor identity are configurable; see the backend contract. IDs are correlation, not authentication. Events contain sequence, timestamp, elapsed time, effective configuration version and route. Stages include received, inspected, upstream_selected (per attempt), connect_failed, upstream_request_prepared, request_body_complete, response_headers and completed/blocked/failed. Background analysis follows terminal events. Partial responses can retain status 200 and still fail.

Lifecycle SQLite uses a dedicated writer, WAL, cached statements and immediate batches of up to 32 completed journeys. Each captured request reserves one channel slot at admission; phases accumulate in bounded memory and live telemetry still updates immediately. With control disabled, capture stays request-local and does not lock the active-journey registry; completed phases move into the writer without deep clones. A request with analysis retains its slot through analysis, then submits phases and result together. SQLite write failures retain the bounded batch and retry; required admissions close while storage is unhealthy. No SQL write or fsync is awaited by forwarding.

`set_persistence {admission="required", synchronous="full"}` is the default. Required capture saturation returns 503 before forwarding. `best_effort` permits forwarding without a capture reservation and counts the omitted batch. FULL synchronizes committed WAL transactions; NORMAL trades power-loss durability for fewer syncs. Neither setting protects work that has not reached a database commit. Graceful shutdown drains accepted work where possible; persistent disk failures cause a nonzero shutdown result instead of a false success.

Retention uses an indexed request-to-last-event table and deletes entire older journeys after at least 1024 inserted events. It is a soft event target; batch size and straddling journeys can temporarily exceed it. Existing event rows are indexed at startup. The event writer sets a 1 GiB main-database page budget; WAL/disk overhead is additional. The lifecycle, verdict and analysis connections share one FIFO write coordinator. WAL reads bypass this coordinator. Numeric journal preparation/completion group up to 32 ready jobs per transaction without a batching timer, reducing sync calls and avoiding competition among AegisX writers. External writers and slow disks can still delay work. Queue and database budgets remain necessary on finite hardware.

The analysis journal retains up to roughly 10,128 finished numeric records plus pending work. Recovery never recreates bans, cancellation or webhook actions. Text-input models, incompatible hashes and invalid vectors are marked unavailable. There is no exactly-once backend-effects guarantee. Observe-only recovery does not reconstruct an uncommitted HTTP journey.

New databases use owner-only permissions. Captured data excludes bodies, raw path/query, cookies, credentials and header values; direct peer IP and numeric admission features are retained. Active phases and raw queue samples are volatile; a crash before a completed journey commit can leave no persisted journey. The live panel is not a durable acknowledgement.

## Build, checks and actual scope

```sh
# Build panel/out first (python3 ../scripts/build.py from server/ also resolves the root).
cargo test --locked
cargo clippy --locked --all-targets -- -D warnings
cargo build --release --locked
AEGISX_BIN=target/release/aegisx python3 tests/lifecycle.py
AEGISX_BIN=target/release/aegisx python3 -m unittest discover -s tests -p "*.py" -v
```

`core` owns shared errors/time/numeric facts; `module` owns config, runtime, identity, memory, decisions, lifecycle collection, durable verdicts, cache, inference, upstream, storage, telemetry, webhooks, control and proxy adapters. Services composes shared dependencies once; app owns startup/CLI/shutdown. One crate, no Cargo workspace. Handwritten agentx-style spacing is intentional.

Model, ONNX CPU runtime, Lua, SQLite and OpenSSL are embedded/statically linked; the GNU executable still needs standard Linux C/C++ libraries. No Python/Node production runtime or companion model library is required. Verify `ldd` for your target; this is not a universally static executable.

Version snapshot, 2026-10-01: Rust 1.98.1, Pingora 0.9.0, Tokio 1.53.1, mlua 0.12.1, rusqlite 0.40.2, arc-swap 1.9.2, sha2 0.11.0, openssl 0.10.81, ort 2.0.0-rc.13. Cargo.lock pins the graph. ort remains a release candidate with bundled ONNX Runtime 1.28.0; Python uses 1.30.0. Parity is tested across both. Native engine versions follow crate bundles.

This increment does not implement DNS discovery, ACME, verified HTTP/2/3 support, WebSocket/CONNECT tunnels, compression, Kubernetes ingress/controller integration, automatic backend instrumentation, durable webhook delivery or arbitrary active-request termination. It establishes useful independent proxy behavior, not complete NGINX/Caddy parity or enterprise readiness.

Design references: [Pingora 0.9.0](https://github.com/cloudflare/pingora/releases/tag/0.9.0), [Caddy reverse_proxy](https://caddyserver.com/docs/caddyfile/directives/reverse_proxy), [NGINX upstream](https://nginx.org/en/docs/http/ngx_http_upstream_module.html). The implemented semantics and limits above are AegisX's own, not an assertion of identical behavior.

Measured local throughput and telemetry overload are documented in [benchmarks/README.md](benchmarks/README.md).

## Text/journey inputs (0.8)

Use set_model {mode="observe",scan_bytes=16384,response_scan_bytes=1024,journey=true}. Response sampling defaults to zero and is an explicit transient-content opt-in. Only textual Content-Type and identity/no Content-Encoding qualify, checked before configured response-header rewrites. Sampling copies a bounded prefix while streaming; it neither buffers the full response nor decompresses content. Text tensors contain up to 1024 bytes per stream. Numeric extraction can inspect the configured larger request prefix.

Journey inference needs authenticated backend reports; the proxy cannot observe arbitrary internal code. Reports retain optional span_id, parent_id and elapsed_ms; model event budgets are 32 events and 64 name bytes each. All input modalities plus artifact identity enter score-cache keys. Analyzed records expose component_scores and model_inputs coverage/truncation, without raw text.

For offline verification, score --input /path/tensors.json accepts one flat tensor object matching model/weights/parity.json. The input file is capped at 256 KiB and every tensor dimension/range is validated. The legacy --features numeric-only command rejects the new embedded model rather than fabricating a text-free verdict.


## Decision integrity and calibrated signals (0.9)

Cached decisions use immutable shared values, including negative lookups. The serial SQLite owner rechecks the cache before a queued read, so concurrent misses reuse an earlier read or committed write. Cached statements reduce SQL preparation. Put/revoke publishes only after SQLite succeeds; a delayed read cannot overwrite a later commit. Read/list validate bounded payload size, row/payload key and expiry agreement, supported sources and field bounds. Invalid records produce a lookup error, then the configured on_lookup_failure policy decides allow/deny. The identity secret must be exactly 32 bytes at startup.

The control API exposes database_reads, coalesced_reads and read_failures alongside existing counters. Direct external database edits are unsupported; explicit cache purge is necessary for controlled diagnostics. Successful API mutations update the cache immediately.

Use set_model {mode="observe", content_threshold=0.9491480588912964, journey_threshold=0.9946383237838745}. A route's threshold overrides both global component values; unspecified values fall back to the global threshold (default 0.95). Missing request/event inputs cannot trigger their absent signal, even at threshold zero. Legacy numeric models use threshold only. Analysis events report effective thresholds and triggered_signals.

The default decision namespace remains compatible with 0.8 when these options are omitted. Explicit component policy changes isolate old decisions. Supported lifecycle-v8 external artifacts remain loadable under their original architecture metadata. Once forwarding begins, new decisions still affect only later requests.
