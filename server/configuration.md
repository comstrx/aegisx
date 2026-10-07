# Context, cache and integrations · 0.9

All settings below belong in the same Lua file. Setters replace their group and restore defaults for omitted fields. Unknown keys fail validation. Explicit settings take priority over model scores. Lua is compiled into immutable runtime settings at startup/reload; no request executes Lua.

## Context and telemetry

```lua
set_context { shards=16, idle_ttl_ms=120000 }
set_telemetry { enabled=true, recent_capacity=256, capture="summary", sample_every=1 }
```

Context keeps ten one-second buckets, observed errors/blocks, in-flight counts, first/last timestamps and previous latency. It never feeds prior model scores back as ground truth. Global actor and route/actor maps have independent `limits.max_sources` bounds. Randomized hash partitioning reduces lock contention. Capacity is divided across shards; an individual full shard may reject a new identity with 503 before the whole map fills. Active identities are never evicted. Idle entries expire on subsequent access or capacity cleanup.

Shards: 1–64 (internally no more than capacity). Idle TTL: 10,000–86,400,000 ms. This is process-local memory; restart resets history.

Telemetry metrics count every request reaching the request filter when enabled with control active. With no control listener, unused live counters/histograms are skipped; configured SQLite sampling still works. `capture="full"` records every supported lifecycle stage; `"summary"` retains inspection/rejection/terminal/deferred-analysis outcomes. `sample_every=N` captures each Nth request's journey; it does not sample enforcement or the context counts underlying feature extraction. Recent capacity is 1–4096; sample interval 1–10,000. Raw body, URL, tokens and actor header contents are never recorded.

`set_store(false)` independently disables SQLite. `telemetry.enabled=false` disables live metrics/history, but configured SQLite capture still works. Route `capture=false` disables that route's events. Storage and live rings are bounded; sampling/retention can make history incomplete.

## Three independent caches

```lua
set_cache {
    decisions=true, scores=true, responses=true,
    max_entries=10000, max_bytes=16777216, max_object_bytes=262144,
    deny_ttl_ms=5000, score_ttl_ms=1000, response_ttl_ms=10000,
    background_denials=false,
    lookup_timeout_ms=25, write_timeout_ms=1000, decision_ttl_ms=1000,
    on_lookup_failure="deny"
}
add_route { name="personal", path="/account", response_cache=false }
add_route { name="shared", path="/health", model="off", decision_cache=false }
```

All three caches default off. Decision enforcement requires `set_store`: SQLite is authoritative. A positive or negative cache miss reads the repository; successful commits publish cache changes. Revoking a decision removes it from SQLite and updates the cache. Purge only evicts cached state; it never revokes persistent decisions. Writes must go through this repository/API; direct external SQL edits are unsupported.

| Cache | Key / eligibility | Effect |
| --- | --- | --- |
| Decisions | Stable policy fingerprint + route + persistent actor handle | Cache hit or bounded SQLite lookup; 403 before forwarding |
| Scores | Exact normalized feature float bits + model artifact SHA | Reuse a successful model result, retaining current policy threshold |
| Responses | Cache generation + configuration + route + Host + full URI + Accept-Encoding | Forward a fresh cached public GET response after normal admission checks |

Decision sources are authenticated operator commands, configured backend feedback, or explicitly enabled model denial policies. Model entries only appear for an actual score over the route threshold. Unavailable inference is never cached as a model verdict. `background_denials=true` can affect a later request; it cannot change the completed one. It requires an evaluated artifact or explicit experimental opt-in. Observe mode cannot generate blocks or cancellations. Deny hits do not extend expiry, call inference, repeat webhooks or feed predictions into context.

Score caching is exact; changing history often changes the key. It is not an approximate “same user” shortcut and does not guarantee a useful hit rate.

Response caching requires status 200 and explicit `public` with positive `s-maxage` or `max-age`. Age/Date reduce freshness and the configured TTL caps it. Rejects private/no-store/no-cache, Set-Cookie, Content-Range, WWW-Authenticate, unrecognized Vary and oversized objects. Only Vary: Accept-Encoding is supported. Requests with Authorization, Cookie, Range, conditional/cache-control headers, bodies, duplicate encoding/host fields or a configured actor header bypass it. GET only; partial/error responses are never committed.

Each hit gets the current request's correlation ID and updated Age. Rate and deny policies run before hits. Successful unsafe methods conservatively invalidate all response entries. Purge advances the cache generation, so an older in-flight fill cannot become a hit afterward. Stale serving, revalidation, disk caching and cache-miss request coalescing are not implemented. The backend remains responsible for correctly marking public content.

`max_entries`: 1–100,000 for each decision/score cache. Response weighted budget: 1 KiB–256 MiB, including body/header estimate. Object limit: 1 byte–1 MiB and no larger than budget. TTLs: 1–600,000 ms. Concurrent fill bodies have a separate semaphore budget equal to max_bytes; total process RSS includes both, cache metadata, queued maintenance and other subsystems. Moka eviction is asynchronous, not an exact process-memory cap.

Cache booleans/decision-source toggles can change on SIGHUP. Capacity/TTL/deadline changes require restart. Responses use configuration generations. Decision scopes use a stable fingerprint of relevant route/identity/model/cache settings, so identical settings and the persisted identity key preserve decisions across restart. A changed policy gives a different scope; existing rows remain until expiry. An operator/backend mutation advances a global revision so older queued ML results cannot reverse it.

## Identity, control and webhooks

```lua
set_identity {
    request_id_header="x-correlation-id", propagate=true,
    preserve_trusted_id=false, forwarding=true,
    forwarded_for_header="x-forwarded-for", forwarded_proto_header="x-forwarded-proto",
    backend_block_header="x-backend-ban"
}
-- Only when behind an authenticating trusted gateway:
-- set_identity { actor_header="x-account-id", trusted_peers={"127.0.0.1/32"} }

set_control {
    enabled=true, panel=true, listen="127.0.0.1:9090",
    api_prefix="/api/v1", token_env="AEGISX_ADMIN_TOKEN",
    backend_token_env="AEGISX_BACKEND_TOKEN" -- optional, separate credential
}

set_webhooks {
    enabled=false, queue_capacity=128, timeout_ms=1000, attempts=3,
    endpoints={
        { name="operations", url="https://backend.example.com/aegisx/events",
          secret_env="AEGISX_HOOK_SECRET", events={"backend_signal","risk_detected","blocked","cancellation_requested"} }
    }
}
```

Provide secret values through the service environment. Tokens/signing secrets are supplied through environment variables and are not recorded. The local identity HMAC key is stored in the owner-only SQLite database for stable actor scopes. Read the [complete contracts](contracts/README.md) before implementing a receiver, especially trusted identity and cooperative job cancellation. Webhooks are best effort, not durable. Up to 8 endpoints; queue 1–4096; timeout 100–10,000 ms; 1–5 attempts. Network calls never execute on the proxy request path.

Control binds only loopback and requires authentication for operational data/actions. Routes/token-variable/control listener, webhook endpoints and context resources require restart; listener/resource reload errors preserve the last working configuration.

## More routing and health controls

```lua
add_route {
    name="preview", host="*.example.com", path="/api", methods={"GET"},
    match_headers={ ["x-release"]="preview" }, upstream="preview"
}
set_balancer("preview", {
    policy="adaptive", health_interval_ms=1000, health_timeout_ms=300,
    health_path="/ready", health_status=200, max_fails=2, cooldown_ms=5000
})
```

Exact hosts outrank wildcard hosts, which outrank hostless routes. Longer matching host suffixes take precedence, then longest path, exact path, method specificity, more header conditions, and name. Wildcards match a nonempty subdomain (including deeper subdomains), not the apex or a suffix lookalike. Header conditions are exact values and require one field value; they are routing conditions, not authentication.

Without health_path, active health is TCP liveness only. With a path, probes use HTTP or verified HTTPS according to the backend and expect the configured status (200–599, default 200). Redirects are not followed. Probes share configured address/SNI/CA policy, run outside hot paths and never read an unbounded body. Their application readiness is only as meaningful as the chosen endpoint.

## References and version choices

Reviewed 2026-10-01: [NGINX proxy controls](https://nginx.org/en/docs/http/ngx_http_proxy_module.html), [Caddy reverse proxy](https://caddyserver.com/docs/caddyfile/directives/reverse_proxy), [Kubernetes Gateway API](https://kubernetes.io/docs/concepts/services-networking/gateway/), [Moka cache](https://docs.rs/moka/latest/moka/sync/struct.Cache.html), [Next.js static export](https://nextjs.org/docs/app/guides/static-exports), [Standard Webhooks signing format](https://github.com/standard-webhooks/standard-webhooks/blob/main/spec/standard-webhooks.md).

Added dependency versions were checked against official registries: Actix Web 4.15.0, Moka 0.12.16, reqwest 0.13.5, ipnet 2.12.2, httpdate 1.0.3; Next.js 16.3.8, React 19.3.0, TypeScript 7.0.2, Node 26.10.0. Cargo.lock, uv.lock and package-lock.json pin the tested graph. There is no universal “strongest” library independent of workload.

## Durable decisions and cooperative actions

`lookup_timeout_ms`: 1–100 ms (default 25), only on cache misses. `on_lookup_failure="deny"` returns 503; `"allow"` continues through all explicit rate/body/route checks. It never bypasses those rules. `write_timeout_ms`: 100–10,000 ms (default 1,000) bounds operator replies and SQLite writer contention. A write timeout reports an unknown outcome because an already queued commit may still complete; inspect/retry idempotently. Positive/negative decision-cache TTL is 1–60,000 ms (default 1,000), independently bounded by the verdict's expiry. Neither warm cache reads nor control-plane metrics require database I/O.

```lua
set_model { mode="observe", scan_bytes=16384, max_queue_age_ms=5000, queue_capacity=64 }
add_route { name="jobs", path="/jobs", cancellation=true, cancellation_ttl_ms=60000 }
```

Cancellation requires storage, enabled control and a separate backend token. TTL: 1,000–600,000 ms. The API persists an intent (maximum 10,000 live records); a signed webhook can notify the backend, which can also poll the durable list. The backend alone acknowledges `accepted`, `cancelled`, `too_late` or `unsupported`. Delivery/acceptance do not prove cancellation. Automatic intents require `background` mode and a score above threshold; operator requests work without a model. These never close the original HTTP stream.

Migration from 0.3: `enforce`, model `timeout_ms`, model `on_failure`, and `cache.model_denials` are rejected. Use asynchronous observation/background policy and explicit `background_denials` instead. Feature-schema-v1 model artifacts are incompatible; retrain/export the current schema v3.

## Capture and analysis admission · 0.5

```lua
set_store("aegisx.sqlite3")
set_persistence { admission="required", synchronous="full" }
set_limits { queue_capacity=1024, retention_events=100000 }
set_model { mode="observe", queue_capacity=64, on_overload="reject", max_queue_age_ms=5000 }
```

These are defaults. Storage queue capacity now counts **whole captured requests**, including active reservations, instead of individual phases. Model capacity counts active-request reservations and queued jobs; up to 32 jobs can additionally be in the worker batch. Required storage/model reservations reject saturated admissions with 503 before forwarding. Existing requests complete normally. This bounds memory and preserves admitted capture during ordinary queue pressure; it deliberately trades admission availability for coverage.

For availability-first operation explicitly choose `admission="best_effort"` and/or model `on_overload="skip"`. These allow incomplete capture/analysis and expose separate dropped-batch/job counters. Route capture=false and telemetry sampling still reduce intentional coverage. `synchronous="normal"` permits loss of recent commits on power failure; FULL is the default. Sync changes require restart; admission/overload policies can reload. Existing requests keep their original settings.

`state.storage` exposes enabled/healthy, committed_events, committed_batches, pressure_rejections, dropped_batches, write_retries and used_slots/capacity. Slots include active reservations; committed counters advance after successful SQL commit. Legacy dropped_events is retained as an alias of the dropped-batch count. `state.analysis` adds pressure_rejections, journal_enabled, journal_healthy, journal_us, journal_failures, processing and recovered. `inference_us` excludes journal synchronization; per-event journal_us allocates shared preparation time across the batch, while aggregate journal_us counts preparation work once; queue_ms includes journal preparation delay. Recovery applies no historical enforcement.

A new model cancellation intent checks the operator generation inside the serialized repository writer, closing the check-versus-commit race. Version 0.4 decision fingerprints are preserved for identical settings; the new overload policy does not invalidate existing bans.

References: [SQLite WAL](https://www.sqlite.org/wal.html) and [synchronous durability](https://www.sqlite.org/pragma.html#pragma_synchronous). No finite queue can promise unlimited traffic, zero latency cost and lossless capture simultaneously.

Database writing uses a shared [Tokio FIFO mutex](https://docs.rs/tokio/latest/tokio/sync/struct.Mutex.html) only inside dedicated background workers; hot-path cache reads never acquire it. Lifecycle and numeric-journal batches share synchronization costs.

## Worker and routing controls · 0.6

```lua
set_runtime {
    threads=2, work_stealing=false, accept_tasks=1,
    upstream_keepalive_capacity=128,
    max_in_flight=4096, keepalive_seconds=30,
    pool_idle_seconds=30, write_buffer_bytes=0
}
```

The default scheduler uses a separate single-thread runtime, HTTP connector and upstream pool per worker. Linux SO_REUSEPORT distributes incoming connections. Policy/admission/cache/SQLite/analysis Services remain shared once. Work-stealing remains configurable; benchmark both under your workload. Accept tasks per descriptor: 1–64. Idle upstream pool capacity per worker: 1–65,536; the total bound scales with worker count. With work stealing enabled, one shared connector derives the total from that same per-worker setting. These are resource settings and require restart. Pool capacity is not an active-request limit; use max_in_flight for admission. No universal worker count or throughput is guaranteed on shared hosts.

Up to **1,024** explicit routes are indexed by segment-prefix candidates at configuration load. Host specificity, longest path, exactness, methods, header matches and deterministic naming keep their existing precedence. Lua size/instruction/memory bounds still apply. Each request retains an immutable configuration snapshot. Backend authorities and pool references are compiled once; URI canonicalization runs once at admission. UUID v4 remains cryptographically random, using the library's thread-local CSPRNG instead of an OS entropy call for each request.

The authenticated state API exposes selected effective values under configuration, including worker settings, schema dimension and route policies. Secret values, token environment names, certificate paths and raw Lua are excluded. The panel preview is read-only and deliberately not a complete replacement config.

The runtime also isolates upstream reuse groups by worker when work stealing is disabled. This reduces moving active socket ownership between worker reactors; Separate default connectors also keep Pingora's idle monitoring on that worker's runtime. TLS/address/options continue to participate in the peer's reuse identity. This is an optimization, not a change to retry or certificate verification policy.

The default build enables mimalloc 0.1.52 with secure mode (bundled native 3.3.2). Profiling showed allocator overhead; measured performance and RSS costs are in benchmarks/v06.md. Build with cargo build --release --locked --no-default-features to compare the system allocator. This is a build choice, not a Lua hot-reload knob, and secure allocation does not prove overall application security.

## Bounded admission waiting (0.7)

```lua
set_queue { capacity=256, timeout_ms=1000 }
```

When a request cannot immediately reserve global forwarding, selected upstream, required capture or required analysis capacity, it can wait asynchronously before forwarding. This is separate from the completed-analysis job queue. The global bound counts active wait episodes across all four resources; it does not buffer entire request bodies or replay completed HTTP operations.

- Library defaults: capacity=0 (disabled), timeout_ms=1000. The supplied Aegisx.lua explicitly enables 256 waiting slots.
- capacity: 0–8192; timeout_ms: 1–30000. Both require restart. A request shares one deadline from its first wait across later admission waits; it never receives a fresh timeout at every resource.
- Immediate capacity uses the existing fast path. Full waiting slots, timeout or unavailable required storage/analysis produce pre-forward 503. A queue cannot guarantee acceptance under unbounded load or a failed backend.
- Upstream capacity wakes on lease release and active-health recovery. Passive quarantine waits until the earliest eligible cooldown; there is no per-connection polling loop during outages. Independent resource queues do not promise one global FIFO order.
- Requests retain their original configuration snapshot. Durable actor restrictions are rechecked after waiting and immediately before upstream headers are prepared. A decision arriving after forwarding begins still affects future admissions only.
- Waiting futures release their slot when cancelled. Client disconnection is not actively polled during resource waits; release can take until resource readiness, timeout or task shutdown.
- Waiting requests are live, volatile connections. Process termination does not replay them. Durable accepted-job processing and cooperative backend cancellation remain separate contracts.
- /state exposes capacity, waiting, timeout_ms, entered, completed, resumed, full, timed_out, unavailable, cancelled and total_us. Totals are wait episodes, not unique HTTP requests; one request may wait on multiple resources. Mean wait uses completed episodes, including cancellation. Independent atomic snapshots can briefly be inconsistent during concurrent updates.
- Queue events include their resource and acquisition outcome in the real request journey. Required storage reservation may itself precede the ordinary received event.

Keep this budget below the latency your clients tolerate. Capacity adds sockets and retained request metadata; it is an availability/latency/memory tradeoff, not free throughput.

### Write buffering and streaming

Version 0.10 defaults write_buffer_bytes to 4096. The pinned Pingora transport patch flushes every already-ready HTTP response batch, including header-only and non-final body batches. It combines writes that are already available; it does not wait for future chunks or buffer whole responses. Zero disables the buffer. A deterministic delayed-chunk test verifies both Content-Length and chunked responses. The older 0.9 buffering delay is fixed. This correctness change does not by itself establish a throughput gain.

Global admission uses an exact atomic counter with cancellation-safe wakeups. Waiting remains bounded by the existing capacity and deadline. It does not promise strict FIFO scheduling. Disabling the control plane skips upstream active/latency bookkeeping only when no balancing policy or backend capacity limit needs those counters. Explicit capacity limits always remain enforced.

Custom header names compare case-insensitively. Route values override global values regardless of spelling case; duplicate names within one scope are rejected during validation. Header names and values compile once into the immutable routing snapshot.


## Text and journey analysis (0.8)

set_model { mode="observe", scan_bytes=16384, response_scan_bytes=1024, journey=true }

- scan_bytes: request sample budget, 0–65536. Zero disables request text and content statistics; the model can still use separately enabled response/events.
- response_scan_bytes: 0–1024, default 0. Explicit volatile response-text sampling. Binary, compressed or missing Content-Type responses are unavailable to the text input; no decompression.
- journey: default true. Pass authenticated bounded backend operation events into the model. False disables that input; backend observability/capture remains independently configured. Per-route model="off" disables analysis completely.
- Input token budgets are fixed by byte-journey-v1. Response text never enters SQLite. Data can contain secrets in volatile RAM; no universal redaction or secure-erasure guarantee.
- The model emits content, journey and combined=max(content,journey) signals. Existing route/global threshold applies to the combined signal. Explicit threshold values override any reported model recommendation. The supplied 0.95 is an experimental observation threshold, not the independently calibrated head thresholds.
- The score cache hashes every numeric value, request/response token, event value/name and coverage flag plus the artifact/schema. Different responses or backend journeys cannot alias solely because request statistics match.
- The numeric analysis journal remains useful for audit, but cannot recreate interrupted text-model work. Such rows become recovery_unavailable with null score, and never replay bans/webhooks/cancellations. state.analysis.recovered counts reconciled interrupted rows, including unavailable rows.


## Separate risk thresholds (0.9)

```lua
set_model {
    mode = "observe",
    content_threshold = 0.9491480588912964,
    journey_threshold = 0.9946383237838745,
    response_scan_bytes = 1024,
    journey = true
}
```

Both component thresholds are optional finite values in [0,1]. Precedence is route.threshold, then the component-specific global value, then model.threshold. A missing modality never triggers a classification. Legacy numeric artifacts use model/route.threshold. Recommendations are calibration results on this experiment, not production guarantees; explicit values override them. Analysis records show effective thresholds and which signal crossed its limit. Lowering a threshold increases sensitivity and can increase false positives.

Queued cold lookups are coalesced on the serial SQLite owner; cached data still comes from acknowledged database state. Integrity failures follow on_lookup_failure. The decision API reports database_reads, coalesced_reads and read_failures; no invalid durable record is cached as a successful lookup.
