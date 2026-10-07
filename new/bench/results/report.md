# AegisX new/server — engine report (M0 → M4), 2026-10-02/03

Host: i5-10210U (4 cores / 8 threads), WSL2 5.15, nginx 1.24, wrk -t2, 12 s × 4 trials, medians.
Backend: `bench/backend` (C epoll, 128-byte body, `Connection: keep-alive`) — 185k rps when hit directly.
Harness: `bench/compare.py` (CPU from /proc, PSS, context switches), `bench/syscalls.py` (strace -c),
`bench/profile.sh` (perf, user space only: perf_event_paranoid=2), `bench/quiet.sh` (waits for an idle box).

## M0 — engine decision (8 workers, 128 connections)

| target                      | req/s   | CPU µs/req | ctx sw/req | PSS MB |
|-----------------------------|--------:|-----------:|-----------:|-------:|
| direct backend              | 184 891 |        8.2 |       0.06 |    3.5 |
| nginx                       |  66 531 |       54.7 |       0.28 |   59.4 |
| bare Pingora 0.9 (no logic) |  37 296 |      129.6 |       0.57 |   24.3 |
| AegisX hyper, first cut     |  52 164 |       66.2 |       0.63 |   27.8 |
| AegisX hyper, tuned         |  55 413 |       62.9 |       0.63 |   30.1 |

4 workers, 128 connections (same box): AegisX tuned 73 294 rps @ 51.2 µs/req vs nginx 60 485 rps @ 58.8 µs/req.

**Pingora verdict.** A Pingora proxy with no AegisX logic at all costs 2.4× nginx's CPU per request here and
caps at ~37k rps; the old build's "40 % of nginx" was the framework, not the policy layer. What that CPU bought:
Cloudflare-grade connection management, upgrades, zero-downtime restarts and an H2/TLS stack we were not using —
paid for with a work-stealing multi-thread runtime, boxed per-request session objects and header copies on every
hop. The hyper 1.11 + tokio thread-per-core engine keeps the keep-alive pooling and reaches nginx-class CPU.

Oversubscription: 8 workers on 4 physical cores doubles context switches per request (0.63 vs 0.27 for nginx's
8 processes) and costs ~10 µs/req; the default worker count is therefore the number of physical cores.

## Syscalls per request (strace -c, 4 workers, 64 connections)

| target | syscalls/req | breakdown                                   |
|--------|-------------:|---------------------------------------------|
| nginx  |         5.19 | recvfrom 2, writev 2, getsockopt 1, epoll_wait 0.14 |
| AegisX |         4.18 | recvfrom 2, writev 1, sendto 1, epoll_wait 0.15     |

tokio already issues no speculative EAGAIN reads; an AsyncFd short-read adapter measured nothing and was reverted.
The remaining gap to nginx is user-space CPU, not syscalls.

## M1 — routed proxy (Lua config, routing, balancer, identity, limits, TLS), 4 workers

| build                                  | req/s c128 | CPU µs/req | user µs | sys µs | req/s c256 |
|----------------------------------------|-----------:|-----------:|--------:|-------:|-----------:|
| nginx (same run)                       |     64 906 |       50.5 |    18.0 |   32.3 |     66 041 |
| AegisX routed (m1-routed-4w)           |     69 708 |       54.9 |    23.7 |   31.0 |     71 574 |
| AegisX + in-flight quota + body timer  |          — |       55.8 |    24.2 |   31.2 |          — |

Routing, identity, balancer bookkeeping and the per-request guard cost ~3.7 µs CPU per request over the bare
engine (−4.9 % rps); the quota ticket and the response idle timer added ~0.5 µs. Our system time already matches
nginx; the gap is user time (23.7 vs 18.0 µs).

8 workers (oversubscribed): AegisX 53.9k @ 65.5 µs (user 33.0) vs nginx 61.2k @ 51.4 µs (user 20.2) — our user
time grows 40 % from 4 to 8 workers while nginx's grows 12 %: shared cache lines on the request path. The hot-path
pass that followed removed every shared refcount touch per request (epoch-checked per-worker config view instead
of ArcSwap load_full, borrowed route/pool, index-based backend picks, static header names/values). Its clean
re-measurement is pending: the laptop was shared with other sessions for the rest of the day (load 6–12, nginx
itself fell from 65k to 30k rps), so m1-hotpath/m1-limits/m2-observe numbers are not comparable and are not reported.

## Where the user time goes (perf, 4 workers, routed build, user space only)

| share | component |
|------:|-----------|
| ~14 % | our handler and client state machines (route, identity, pool checkout, header rewrites, inlined) |
| ~8.5 % | hyper HTTP/1 parse/encode on both hops (httparse, write_headers, buffered io) |
| ~7 % | allocator (mimalloc) |
| ~5.5 % | memmove |
| ~5 % | hyper client dispatcher (SendRequest → connection task via mpsc + oneshot callback) |
| ~4 % | tokio LocalSet scheduling, ~2.5 % wakers, ~2.2 % the per-request upstream timeout (timer wheel) |
| ~3 % | clock reads (vdso + libc clock_gettime) |
| ~2.2 % | hop-by-hop header stripping (two passes per request; the backend sends `Connection: keep-alive`) |
| ~2 % | hyper body channels (Incoming + Sender per hop) |

The structural costs — hyper's client dispatcher, body channels and LocalSet — are hyper/tokio design, about a
quarter of user time. Beating nginx's user time at 8 workers would need a lean HTTP/1 upstream client that talks
to the pooled connection directly instead of through hyper's `SendRequest`; everything in our own code is already
below 3 % per item. A dwarf call-graph run (`profile-routed-4w-graph.txt`) attributes memmove and allocations.


## Inclusive attribution (dwarf call graph, 4 workers, `profile-routed-4w-graph.txt`)

| inclusive share | where |
|----------------:|-------|
| 47 % | hyper server connection: read, parse, dispatch, encode, write (contains the 19.5 % spent polling our handler) |
| 28 % | hyper client connection task for the upstream hop (dispatcher channel, response parse, request encode, io) |
| 17.6 % | `Handler::handle` (routing, identity, limits, decisions, pool checkout, header rewrites; 8.4 % of it is `Client::send` awaiting hyper) |
| 7.7 % | allocator (mimalloc) |
| 5 % | memcpy |
| ~3 % | clock reads |

Our own logic is about 10 % of user time (≈2.5 µs); hyper's two HTTP/1 state machines are ~55–60 %, the tokio
runtime (LocalSet scheduling, wakers, timer wheel, IO driver) ~20 %.

## Noise, pinning and the observe path (2026-10-03, shared laptop, load 8–25)

The box was never idle again after the M1 measurement, so absolute numbers moved with the background load
(nginx itself: 65k → 30k rps across runs). Interleaving targets inside one run keeps the comparison fair:

| 4 workers, c128, interleaved (`m3-ab-4w`) | req/s | CPU µs/req | user µs | sys µs |
|-------------------------------------------|------:|-----------:|--------:|-------:|
| nginx                                     | 59 182 |      54.9 |    19.3 |   35.5 |
| AegisX (telemetry on, default)            | 59 322 |      61.1 |    27.0 |   33.9 |
| AegisX (`set_telemetry { enabled = false }`) | 60 907 |    59.3 |    25.7 |   34.3 |

Same rps as nginx at 4 workers; per-request user time is 6–8 µs higher in this environment (5.6 µs when the box
was quiet). The telemetry counters and capture branch cost ≤ 1.3 µs (within run-to-run noise); everything else
added since M1 (TLS, quota, idle timer, actor memory, decisions) is off the pure path or branch-gated and did not
move the number. Pinning workers is not the cause: `--no-pin` measured 30.1 µs vs 28.9 µs pinned under the same
load. Our user time inflates more than nginx's under a busy HT sibling (23.6 → 27–33 µs vs 18 → 19–23 µs), which
points at cache footprint (16 KiB buffers per hop, tokio task state) rather than instructions.

**Next lever, if nginx parity at 8 workers matters:** a lean HTTP/1 upstream client that writes to the pooled
connection directly (no `SendRequest` dispatcher task, no body channel), and `set_telemetry { enabled = false }`
for pure-forward deployments. Re-measure with `bench/quiet.sh m4` on an idle box before and after.

## Lean upstream client (2026-10-03 afternoon) — nginx parity at the same worker count

hyper's client (`SendRequest` → dispatcher task → body channel) was replaced by `http/upstream`, a direct
HTTP/1.1 client over the pooled connection: reused per-connection buffers, one reusable timer, httparse with
uninitialised header slots, zero-copy header values, hop-by-hop and identity headers applied while serialising,
framing decoders that hand the connection back to the pool on completion. Clock reads went from eight to two per
request, the injected headers no longer allocate, request ids come from a per-worker arena, and the upstream read
buffer stopped reallocating every request. Pinning is off by default (nginx has no affinity either; pinned workers
doubled context switches when oversubscribed).

Interleaved runs on the shared laptop (3 × 8 s, 128 connections; `m4-lean3-*`, `m4-final-*`, `m4-buffers-8w`):

| workers | nginx req/s | AegisX req/s | nginx user µs | AegisX user µs | nginx CPU µs/req | AegisX CPU µs/req |
|--------:|------------:|-------------:|--------------:|---------------:|-----------------:|------------------:|
| 1       | 19 729 / 17 471 | 19 770 / 17 262 | 14.3 / 14.4 | 13.3 / 16.5 | 50.0 / 55.6 | 50.5 / 57.2 |
| 2       | 25 263 / 27 257 | 26 737 / 30 593 | 21.6 / 21.0 | 23.4 / 19.7 | 75.9 / 71.2 | 73.0 / 65.2 |
| 4       | 70 270 / 45 668 | 72 041 / 64 822 | 19.0 / 23.7 | 19.2 / 21.5 | 55.4 / 72.5 | 54.3 / 57.7 |
| 8 (unpinned) | 82 007 / 73 053 | 79 061 / 55 571 | 18.8 / 20.3 | 20.9 / 24.4 | 48.8 / 53.0 | 49.8 / 60.8 |

Two runs per row (first / second) because the background load moved between them. At 1, 2 and 4 workers AegisX
now matches or beats nginx on throughput and CPU per request. At 8 workers (two threads per physical core) results
swing from −4 % to −24 % in throughput with +1 to +4 µs user time; that delta appears only under hyperthread
contention and is the remaining gap. The next candidate is per-connection memory (hyper's 8 KiB read buffer plus
our 16 KiB upstream buffer per connection), to be measured on an idle box with `bench/quiet.sh`.

Profile of the lean build (`profile-lean-4w*.txt`): hyper server side ≈ 45 % of user time, our gateway + upstream
client ≈ 35 % (of which response parsing 10 %, request serialisation 4 %), allocator ≈ 7 %, memcpy ≈ 4 %.

## Batch E — access log, rewrites, static files, compression (2026-10-03 night)

Access log (`app/access`): nginx-style, zero I/O on the request path. One writer thread per process pulls byte
chunks from a bounded channel (1024 chunks) and appends them with a fresh `open(O_APPEND)` per chunk, so
`logrotate` renames need no reopen signal; `path = "stdout"` writes to the console. Each worker buffers lines in
a `Vec<u8>` and hands it over when it reaches `buffer` bytes (64 KiB default) or every `flush_ms` (1 s default);
when the channel is full the chunk is dropped and counted (back-pressure never reaches the worker). Entries are
written when the response body finishes, so `bytes_sent` is what left the socket (counted by the same tap the
analyser uses when it is on). Formats: `combined` = nginx combined + `request_time backend route request_id`,
and `json`. Config: `set_access_log { path, format = "combined" | "json", buffer, flush_ms }`.

Rewrites (`http/rewrite`, `regex` 1.13): `add_route { rewrite = { from, to, status } }` or a list of such
tables. Rules run in order on the request path, nginx semantics: a `to` that starts with `http://` or `https://`
or carries a `status` is a redirect (302 unless given 301/303/307/308), a `to` ending in `?` drops the query
string, a `to` with its own query merges with the request's, and once a rule rewrote the path `strip_prefix`
no longer applies — the changed URI is forwarded whole, as nginx does. Rules run after deny / decisions /
rate limits and before body checks, so blocked actors never see redirect targets. Bad patterns fail at boot.

Static files (`http/files`, `httpdate` 1.0, tokio `fs`): `add_route { root, index, cache_control, strip_prefix }`
(`strip_prefix` turns `root` into nginx `alias`). GET/HEAD only (405 with `Allow`), canonical path +
percent-decoding + a per-component check (`Normal` only, no NUL, no `\`/`:` outside unix), directory → `index.html`
(configurable; `index = ""` disables and yields 403) or 301 to the slashed path with the query preserved, strong
ETag `"mtime_hex-size_hex"` like nginx, `Last-Modified`, `If-None-Match` (weak compare, lists, `*`),
`If-Modified-Since`, single-range `Range` (206 / 416, suffix and open ranges, `If-Range` by ETag or date),
`Accept-Ranges`, built-in media-type table. Bodies stream from `tokio::fs::File` in 64 KiB reads through the
same `Body` enum (`Body::File`), so tickets, access logging and compression apply unchanged. Honest limits:
no `sendfile` (hyper writes from user space), multipart ranges answer with the full body, no autoindex.

Compression (`http/encode`, `flate2` 1.1 on `zlib-rs`, `brotli` 9): `set_compression { enabled, min_bytes = 1024,
level = 4, brotli = true, brotli_level = 4, types = {...} }` plus `compress = true|false` per route. Negotiation
reads `Accept-Encoding` with q-values (br preferred, then gzip, `*` honoured). A response is encoded only when it
is 200/403/404, has a listed media type, no `Content-Encoding`/`Content-Range`, no `Cache-Control: no-transform`,
and either no `Content-Length` or one ≥ `min_bytes`. The filter removes `Content-Length` and `Accept-Ranges`,
adds `Content-Encoding` and `Vary: Accept-Encoding`, weakens a strong ETag to `W/`, and wraps the body: output
is emitted when 16 KiB of compressed bytes accumulate, when the upstream stalls (sync flush, so SSE and slow
streams keep flowing) and at the end. Compression runs on the worker thread like nginx; it is off by default.

Measurements (4 workers, 128 connections, 3 × 8 s interleaved, box at load 5–7 the whole evening; `m6-batchE3/4/5-4w`):

| target | req/s | user µs/req | CPU µs/req | PSS MB | note |
|---|---:|---:|---:|---:|---|
| nginx | 66 678 – 69 126 | 18.4 – 19.7 | 54.8 – 56.1 | 30.7 | `access_log off` |
| nginx-log | 60 845 – 65 995 | 19.4 – 20.7 | 54.6 – 58.4 | 31.0 | `access_log combined buffer=64k flush=1s` |
| AegisX | 55 210 – 63 537 | 21.8 – 22.6 | 54.6 – 60.0 | 29.8 | access log off, compression off |
| AegisX + compression on, no Accept-Encoding | 64 472 | 22.1 | 59.2 | 29.8 | same binary (`m6-batchE3`): the switch costs nothing |
| AegisX + access log | 51 011 – 58 310 | 24.2 – 26.1 | 60.1 – 64.3 | 36 – 41 | `combined`, 64 KiB buffer, 1 s flush |

The access log went from +3.6 µs to +2.4 µs user per request after two passes (owned field buffer instead of
hyper's header slices, direct byte writers instead of `core::fmt`); nginx's own buffered log costs +0.5 to
+2.1 µs across runs, so the two logs are now in the same band. PSS rises ~10 MB with the log on (64 KiB chunks
crossing threads), nginx's by 0.4 MB — next lever is a buffer return channel. CPU per request stays level with
nginx (lower sys, higher user); the user-time gap of 2–3 µs vs. earlier parity runs could not be separated
from the background load (user time of the same binary moved ±1.5 µs between trials); the flat profile shows no
new hot symbol from this batch, only memmove up ~1.5 points. Re-measure on an idle box.


## Batch F — hot-path diet, the rest of the nginx list, HTTP/3 (2026-10-04)

Hot path. The per-request service future that hyper copies into its per-connection slot was 7 408 bytes; it
is 3 776 after boxing the cold awaits (upstream connect / TLS / h2 handshakes, h2 exchange, static files,
request and response buffering), boxing the forwarded request and the analyser state, and keeping trace-only
strings out of the generator. hyper's per-request header-read timer is gone: a sweeper task per worker closes
connections idle past `server.header_timeout_ms` (before the first request) or `server.keepalive_timeout_ms`
(between requests) from two `Cell`s the ticket stamps, so a request costs no timer at all. A single catch-all
route skips the radix tree. The Connection token list and the response header map no longer allocate per
request. Measured on the same noisy box right after the diet (4 workers, 3 × 6 s interleaved): user time
22.3 → 20.6 µs/req with nginx at 19.3 µs in the same run, CPU/req 55.9 (AegisX) vs 57.8 (nginx).

Unix sockets: `add_upstream(pool, "unix:/run/app.sock")` and `set_listen_unix("/run/aegisx.sock")`
(served next to TCP, fanned out from worker 0 through the same generic accept path). TLS over a unix upstream
is refused at validation.

Balancing: `ip_hash`, `hash` with `hash_key = ip | uri | header:<name> | cookie:<name> | query:<name>`
(weighted rendezvous hashing with a fixed seed, so every worker and every restart picks the same backend and a
backend failure moves only its own keys), and `set_sticky(pool, { cookie, ttl_ms, path, secure, http_only,
same_site })` (cookie value = stable 16-hex id of the backend; a sticky backend wins while it is up).

Response buffering: `limits.buffer_responses` / `response_buffer_bytes` (1 MiB) and route `buffer_response`;
responses that fit become one `Content-Length` body, larger ones keep streaming after the buffered prefix.

Static files: a shared byte-weighted cache (quick_cache S3-FIFO) holds files ≤ `max_file_bytes` (4 MiB) in
memory, revalidated by `stat` every `valid_ms` (1 s); hits are served zero-copy from `Bytes`, including
`multipart/byteranges`; larger files stream as before. `set_files { cache_bytes, cache_items, max_file_bytes,
valid_ms }`.

Proxy cache: `set_cache { enabled, capacity_bytes (128 MiB), items, max_object_bytes (1 MiB), valid_ms = {
["200"] = ms, any = ms }, stale_ms, lock }` and route `cache`. GET/HEAD only, bypass on Authorization and
Range; TTL from `s-maxage` > `max-age` > `Expires` > `valid_ms`; never stores `Set-Cookie`, `Vary`,
`Content-Range`, `no-store`/`private`/`no-cache` or objects over the limit. The fill is the same tap that feeds
the analyser and the access log, stored when the body completes; hits carry `Age` and `X-Cache: HIT`, answer
conditional requests with 304, and `stale_ms` + `lock` give stale-while-revalidate (one request refreshes, the
rest get `X-Cache: STALE`). Control API: `GET /api/v1/cache`, `DELETE /api/v1/cache` with `{ "key" }` or
`{ "all": true }`.

HTTP/3: `set_http3 { enabled, listen, max_idle_ms, max_streams, alt_svc_max_age }` on quinn 0.11.12 + h3 0.0.8
(the only pre-1.0 crates in the tree, admitted because the full nginx list was ordered). One QUIC endpoint per
worker with SO_REUSEPORT on Linux, the TLS acceptor's certificates with ALPN `h3`, request bodies streamed as
`Body::Quic`, `Alt-Svc` on every response. Known gap: certificate rotation does not reach live QUIC endpoints.

Variables: header values in `set_headers` and route headers accept `$remote_addr $remote_port $host $scheme
$request_id $request_uri $uri $args $upstream_addr $server_port $msec`; static values cost nothing, dynamic
ones are rendered per request; unknown names fail at boot.

Model DSL: `Model::assist("lifecycle")?.json(&facts)?.threshold(0.7).verdict()` or `.detach().await` runs
the ONNX model off the worker thread and returns `Assessment { scores, risky, dominant, threshold }`; the
builder also takes raw features, named features, text streams, events and coverage.

Measurements (128 connections, 3 × 8 s interleaved per row, box at load 3–7; `m8-*`). Medians of three trials;
the same binary moved ±20 % between runs at 4 workers, so read CPU µs/req and the paired rows, not absolute rps.

| mode | workers | nginx req/s | AegisX req/s | nginx CPU µs/req (user) | AegisX CPU µs/req (user) |
|---|--:|--:|--:|--:|--:|
| proxy | 1 | 14 367 | 15 671 | 66.4 (19.8) | 63.9 (18.9) |
| proxy | 2 | 29 696 / 21 980 | 28 303 / 21 966 | 66.4 / 63.8 (18.7 / 20.6) | 69.7 / 64.6 (21.2 / 24.8) |
| proxy | 4 | 53 441 / 72 084 | 63 617 / 54 722 | 57.5 / 56.5 (19.6 / 19.4) | 60.0 / 58.4 (23.1 / 22.5) |
| proxy | 8 | 46 255 | 62 729 | 62.1 (23.6) | 59.4 (27.5) |
| proxy_cache hit | 4 | 89 343 | 100 275 | 40.9 (16.3) | 28.2 (13.7) |
| static 128 B, first run | 4 | 123 708 | 74 685 | 27.0 (12.8) | 59.9 (22.4), 1.9 ctx sw/req |
| static 128 B, after the fix | 4 | 93 575 | 83 134 | 26.8 (12.4) | 27.1 (13.1) |
| proxy + access log | 4 | 56 350 (nginx-log) | 56 938 | 62.3 (22.2) | 66.7 (25.9) |
| proxy + compression on, no Accept-Encoding | 4 | 59 481 | 59 717 | 58.9 (20.0) | 61.7 (23.7) |

Reading: AegisX beats nginx at 1 and 8 workers (throughput and CPU per request), is level at 2 workers, and
at 4 workers wins or loses by ±20 % depending on the minute while carrying 2–3 µs more user time per request
(its sys time is 1–3 µs lower, so CPU per request is within 2 µs). The cache-hit path is 30 % cheaper per
request than nginx's proxy_cache. Static files went from 2.2× slower (one blocking `stat` per request for
directory → index resolution, 1.9 context switches per request) to parity on CPU and −11 % on throughput after
caching that resolution. The access log costs the same as nginx's (+2.3 vs +2.2 µs); compression switched on
costs nothing until a client asks for it. What is left of the user-time gap sits inside hyper's HTTP/1 server
machinery (≈45 % of user time in every profile) and tokio's scheduler; the proxy's own code is ≈20 %. Beating
nginx on user time in every mode means either an own HTTP/1 server loop in `http/server` (as was done for the
upstream side, where it bought ≈8 µs/req) or accepting hyper's share — a decision for the owner, not a tuning.


## AWS, round 0 — real hardware, upstream back on hyper (2026-10-04)

Cluster: three `c7i-flex.large` (2 vCPU each; the account's Free Plan refuses larger types) in eu-north-1, proxy /
load generator / backend on separate nodes, 10 s × 3 interleaved trials, `bench/aws/`. A request costs ≈ 9 µs CPU
end to end on this hardware; the WSL2 laptop was inflating every number about six times.

| run | workers | conns | nginx req/s · CPU µs/req (user) | AegisX req/s · CPU µs/req (user) | AegisX p99 / PSS |
|---|--:|--:|---|---|---|
| baseline, httparse upstream client | 1 | 128 | 111 746 · 8.9 (3.4) | 108 460 · 9.2 (3.9) | 1.45 ms / 33 MB |
| baseline | 1 | 512 | 101 221 · 9.9 (3.7) | 95 028 · 10.5 (4.4) | 9.6 ms / 60 MB |
| baseline | 2 | 128 | 153 132 · 13.0 (4.4) | 147 074 · 13.6 (5.2) | 1.6 ms / 36 MB |
| baseline | 2 | 512 | 139 604 · 14.3 (4.6) | 135 842 · 14.7 (5.5) | 6.0 ms / 60 MB |
| round 0, hyper `client::conn` upstream | 1 | 128 | 113 349 · 8.8 (3.1) | 82 210 · 12.2 (6.7) | 1.8 ms / 36 MB |
| round 0 | 1 | 512 | 98 465 · 10.1 (3.6) | 48 471 · 20.5 (8.9) | 16.7 ms / 61 MB |
| round 0 | 2 | 128 | 155 128 · 12.9 (4.1) | 108 840 · 18.1 (8.9) | 2.9 ms / 38 MB |
| round 0 | 2 | 512 | 134 337 · 14.8 (4.8) | 107 098 · 18.3 (8.7) | 10.4 ms / 69 MB |

Reading: with its own httparse-based upstream client AegisX sits at 94–97 % of nginx with the same 4 syscalls
per request; moving the upstream side onto hyper's client connections costs 3 µs of user time per request and
halves throughput at 512 connections. `strace -c` shows the same syscalls (1 writev, 1 sendto, 2 recvfrom per
request) and `ss` shows no reconnects, so the cost is hyper's dispatcher design — a channel, a oneshot and a body
channel per request plus the second `HeaderMap` — which hyper-util's own client carries as well. Lessons folded in
anyway: pooled hyper connections must be polled before `try_send_request` (the dispatcher accepts a request only
after it signalled "want"), responses that are already buffered are pulled inside the exchange so small bodies cost
no extra wake, finished connections are retried as stale, upgrades need `with_upgrades()` on a spawned link.

Memory: 25 MB PSS idle, +4 KB per idle connection, +68 KB per connection that served a request (hyper keeps an
8 KiB read and an 8 KiB write buffer on each side of the proxy, nginx returns its pool buffers after every
request). The engine rule settles it: hyper on both sides, no hand-written protocol code; the gap is closed by
how hyper is used (see the second-session rounds below).

## AWS, round 1 — routing parity features, static and cache modes measured (2026-10-04)

Round 1 shipped the routing gaps against nginx/Caddy: regex locations (`regex`, with `exact` and `prefer` = `^~`
stopping the regex scan, host-scoped including `*.` suffixes, config order wins), `try_files` (`$uri`, `$uri/`,
literal paths, `=code`, `@upstream` hands the request to the route's pool; non-GET goes straight to the upstream
when it is the fallback), `error_pages` (route-level and global `set_error_pages`; pages served from any static
route or redirected for absolute URLs; `intercept_errors` opts upstream statuses in; 401/405/503 keep
`WWW-Authenticate`, `Allow`, `Retry-After` and the request id), `redirects` (`proxy_redirect`: default strips the
upstream authority back to the route mount, explicit prefix rules, `Refresh` handled too), `cookie_domain` /
`cookie_path`, and `backup` / `down` backends (backups only serve when no primary is open; `down` never probed).
The file cache now keys directories apart from files (`Key::Dir` / `Key::File`) so `/docs` no longer hits the alias
`/docs/` planted and `try_files $uri` cannot be satisfied by a directory. 154 integration tests, clippy clean.

Same cluster and method as round 0 (2 vCPU nodes, 10 s × 3 interleaved trials, nginx 1.24 with the configs in
`compare.py`). Proxy mode is the hyper-everywhere build the owner is deciding on; static and cache modes are new.

| mode | workers | conns | nginx req/s · CPU µs/req (user) | AegisX req/s · CPU µs/req (user) | AegisX / nginx | AegisX p99 / PSS |
|---|--:|--:|---|---|--:|---|
| proxy | 1 | 128 | 115 509 · 8.6 (3.2) | 84 008 · 11.9 (5.9) | 73 % | 1.7 ms / 31 MB |
| proxy | 1 | 512 | 102 923 · 9.7 (4.0) | 63 719 · 15.7 (7.5) | 62 % | 12.1 ms / 57 MB |
| proxy | 2 | 128 | 152 761 · 12.9 (4.3) | 113 403 · 17.6 (8.6) | 74 % | 2.6 ms / 32 MB |
| proxy | 2 | 512 | 139 391 · 14.3 (4.8) | 105 934 · 18.8 (9.2) | 76 % | 9.8 ms / 63 MB |
| static | 2 | 128 | 274 815 · 7.3 (3.2) | 270 836 · 7.4 (3.5) | 99 % | 0.77 ms / 29 MB |
| static | 2 | 512 | 258 011 · 7.7 (3.2) | 247 778 · 8.1 (3.8) | 96 % | 3.5 ms / 40 MB |
| cache | 2 | 128 | 197 305 · 10.1 (3.9) | 283 389 · 7.1 (3.3) | 144 % | 0.9 ms / 32 MB |
| cache | 2 | 512 | 189 227 · 10.5 (4.0) | 255 412 · 7.8 (3.6) | 135 % | 3.5 ms / 42 MB |

Reading: where no upstream connection is involved AegisX is at nginx (static, 96–99 %) or well past it (cache
hits served from memory, 135–144 %; nginx's `proxy_cache` goes through its disk cache and a `sendfile`-less
copy). The proxy gap is unchanged from round 0 and is entirely user time: 8.6 µs against 4.3 µs per request at two
workers, the same syscall count. Static mode's per-connection PSS grows 22 KB per served connection, proxy mode
another 35 KB for the pooled upstream side; both are hyper's 8 KiB read + 8 KiB write buffers per side plus the
connection task, the local experiment with 8 KiB `server.buffer` / `client.buffer` changed nothing, which confirms
the fixed `INIT_BUFFER_SIZE` in hyper rather than our settings.

## AWS, round 2 — TLS resumption, HTTP/2 and HTTP/3 knobs, first TLS/h2 numbers (2026-10-04)

Shipped: stateless TLS session tickets (aws-lc-rs `Ticketer`, `tls.tickets = true`) plus a sized stateful cache
(`tls.session_cache`, 16 384 default, 0 disables) — the integration test asserts `HandshakeKind::Resumed` on the
second connection for both paths; upstream TLS clients keep 1 024 sessions. HTTP/2 server knobs
(`server.h2_adaptive_window`, `h2_stream_window` 1 MiB, `h2_connection_window` 4 MiB, `h2_max_frame` 16 KiB,
`h2_max_header_bytes` 64 KiB — hyper's default header-list cap was 16 KiB). HTTP/3 transport knobs
(`http3.congestion = cubic | bbr | new_reno`, `stream_window`, `send_window`). `compare.py --protocol h1|h1tls|h2`
(h2load for h2, a per-run self-signed EC certificate, nginx `listen ssl http2` with `ssl_session_cache`) and
`--handshakes` (server CPU per handshake measured around `openssl s_time` runs from the load node).

| mode / protocol | workers | conns | nginx req/s · CPU µs/req (user) | AegisX req/s · CPU µs/req (user) | AegisX / nginx | AegisX p99 / PSS |
|---|--:|--:|---|---|--:|---|
| proxy, h1 over TLS | 2 | 128 | 106 912 · 18.6 (8.9) | 102 700 · 19.4 (10.1) | 96 % | 2.8 ms / 42 MB |
| proxy, h1 over TLS | 2 | 512 | 99 589 · 19.9 (9.6) | 93 929 · 21.1 (10.9) | 94 % | 9.2 ms / 71 MB |
| proxy, h2 (10 streams/conn) | 2 | 128 | 109 037 · 18.2 (9.6) | 101 321 · 19.7 (13.4) | 93 % | 42 ms / 79 MB |
| proxy, h2 (10 streams/conn) | 2 | 512 | 83 806 · 23.9 (12.3) | 44 206 · 44.1 (21.3) | 53 % | 248 ms / 166 MB |
| static, h2 | 2 | 128 | 236 162 · 8.5 (5.8) | 228 940 · 8.5 (7.8) | 97 % | 12 ms / 55 MB |
| static, h2 | 2 | 512 | 230 838 · 8.6 (6.0) | 197 722 · 9.8 (8.8) | 86 % | 44 ms / 104 MB |

TLS handshakes (server CPU per handshake, EC P-256 certificate, `openssl s_time` client on the load node): nginx
321–336 µs, AegisX 152–167 µs — rustls on aws-lc-rs costs half of OpenSSL 3.0 here. The `-reuse` column of
`s_time` does not exercise TLS 1.3 resumption (it closes before reading the ticket), so resumed-handshake CPU is
only proven by the unit test for now. h2load reports no percentiles: the p50 column holds its mean, p99 its max.

Reading: once TLS is in the picture the h1 proxy gap closes to 4–6 % (TLS work dominates and ours is cheaper per
byte), h2 at 128 connections is at 93 %, static over h2 at 97 %. The two bad rows share one cause, found with
`strace -c` on the proxy (`diag512.strace`, `diagh2.strace`): **upstream connection churn**. The idle pool keeps
256 connections per worker and backend; with 512 busy clients (h1) or 5 120 concurrent streams (h2) the number of
idle upstream connections oscillates above that cap, finished connections are closed and the next wave opens new
ones — 3 400 `connect` in 5 s at h1/512c/1 worker, 24 290 in 5 s at h2/512c (15 % of requests), each with
`socket`, `connect`, two `epoll_ctl`, `getsockopt`, `setsockopt`, `close`, plus 16 k sockets in TIME_WAIT. That is
the doubled sys time and the 5× context switches in the h2/512c row; nothing in the request path itself regresses.
Per-connection memory is unchanged: a 4 032-byte connection task, hyper's 8 KiB read and 8 KiB write buffers
per side, and the boxed 3.7 KB service future.

h3 cannot be benchmarked on this cluster: the Ubuntu 24.04 `h2load` lacks QUIC, nginx 1.24 lacks `http_v3`, curl
8.5 lacks HTTP/3. The h3 path is covered by tests only.

## AWS, round 3 — operations parity: auth, limits, metrics, resolver, log formats, and the pool churn fix (2026-10-04)

Shipped: Prometheus `GET /api/v1/metrics` (same Bearer auth as the rest of the control API), per-actor
`concurrency_limit` (503, global and per route), `bandwidth` / `bandwidth_after` (bytes per second per response,
`Body::Paced` token bucket in 50 ms slices, applied once in `Handler::handle` so the hot path pays one bool),
`basic_auth` (bcrypt via `spawn_blocking`, `{SHA}`, `{PLAIN}`, `users_file`, a 60 s cache of accepted
`Authorization` values keyed by SHA-256), `forward_auth` (GET subrequest through the same `Proxy::forward` path
with `X-Forwarded-Method/Uri/Host`, non-2xx returned verbatim, `copy_headers` override whatever the client sent),
custom access log patterns (`set_access_log { pattern = "$remote_addr ... $http_x_tenant ..." }`, nginx variable
names, up to eight captured request headers, `$ssl_client_s_dn`), named backends (`add_upstream("api",
"backend.internal:8080")` resolves at boot and every `client.resolve_ms`, one backend per address, TLS SNI defaults
to the name) and the churn fix from round 2: `Pool::put` keeps a connection beyond `pool_capacity` while the oldest
idle one is younger than two seconds, bounded at four times the capacity (`pool.rs`).

Full matrix on the same cluster (2 workers unless noted, 10 s × 3 interleaved trials, 128 / 512 connections):

| mode / protocol | workers | conns | nginx req/s · CPU µs/req (user) | AegisX req/s · CPU µs/req (user) | AegisX / nginx | AegisX p99 / PSS |
|---|--:|--:|---|---|--:|---|
| proxy, h1 | 1 | 128 | 113 672 · 8.8 (3.6) | 80 577 · 12.4 (6.7) | 71 % | 1.9 ms / 30 MB |
| proxy, h1 | 1 | 512 | 101 082 · 9.9 (4.1) | 69 080 · 14.5 (6.7) | 68 % | 10.9 ms / 59 MB |
| proxy, h1 | 2 | 128 | 151 307 · 13.2 (4.4) | 111 286 · 18.0 (9.0) | 74 % | 2.4 ms / 34 MB |
| proxy, h1 | 2 | 512 | 138 791 · 14.4 (4.7) | 102 947 · 19.4 (9.5) | 74 % | 9.8 ms / 65 MB |
| static, h1 | 2 | 128 | 269 853 · 7.4 (3.1) | 271 356 · 7.3 (3.5) | 101 % | 0.9 ms / 30 MB |
| static, h1 | 2 | 512 | 256 277 · 7.8 (3.2) | 250 764 · 8.0 (3.7) | 98 % | 3.4 ms / 39 MB |
| cache hit, h1 | 2 | 128 | 200 328 · 10.0 (3.7) | 280 858 · 7.1 (3.3) | 140 % | 0.7 ms / 30 MB |
| cache hit, h1 | 2 | 512 | 188 119 · 10.6 (4.0) | 251 054 · 8.0 (3.6) | 133 % | 3.6 ms / 44 MB |
| proxy, h1 over TLS | 2 | 128 | 109 636 · 18.2 (8.7) | 98 486 · 20.2 (10.7) | 90 % | 3.2 ms / 41 MB |
| proxy, h1 over TLS | 2 | 512 | 97 670 · 19.9 (9.6) | 95 449 · 20.7 (10.7) | 98 % | 11.1 ms / 73 MB |
| proxy, h2 | 2 | 128 | 106 631 · 18.7 (9.8) | 102 321 · 19.5 (13.2) | 96 % | 32 ms / 99 MB |
| proxy, h2 | 2 | 512 | 83 292 · 24.1 (12.1) | 73 197 · 27.2 (18.4) | 88 % | 155 ms / 223 MB |

Reading: the churn fix turned the two collapsed rows of round 2 into 88 % (h2, 512 × 10 streams, was 53 %;
`strace -c` now shows 18 connects in five seconds instead of 24 290, two syscalls per request) and 68–74 % (h1
single worker at 512 connections, was 62 %; sys time 7.3 µs against nginx's 5.9 instead of 8.5), at the price of
a larger idle pool (223 MB PSS at 5 120 streams). Plain-HTTP/1 proxying stays at 71–74 %: the gap is the extra
~4.5 µs of user time per request that the round-2 profile attributes to hyper's client dispatcher, the second
header map and the per-request allocations around it; every other mode is at or above nginx. Run-to-run noise on
this cluster is about ±3 % at 128 connections and ±8 % at 512.

## AWS, round 4 — zstd, mutual TLS, OCSP staples, ACME auto-HTTPS (2026-10-04)

Shipped: zstd compression (`zstd 0.14`, preferred over brotli when the client offers it, `compression.zstd`,
`zstd_level`), mutual TLS (`tls.client_ca`, `tls.client_auth = optional | required` through rustls'
`WebPkiClientVerifier`; the verified client certificate is parsed once per connection and exposed as
`$ssl_client_verify`, `$ssl_client_s_dn`, `$ssl_client_i_dn`, `$ssl_client_serial`, `$ssl_client_fingerprint`
(SHA-256) for header templates and `$ssl_client_s_dn` in access patterns), OCSP stapling from files
(`tls.ocsp`, `add_certificate { ocsp }`, the nginx `ssl_stapling_file` model — no responder fetching yet), and
ACME (`instant-acme 0.8.5` + `rcgen`): `set_tls { acme = { domains, email, directory | "staging" | "production",
directory_ca, cache_dir, challenge = "tls_alpn_01" | "http_01", listen, renew_days } }`. The listener comes up
immediately on a self-signed placeholder, worker 0 orders in the background (1 min → 1 h backoff, re-checked every
6 h, renewal when `renew_days` remain), tls-alpn-01 is answered by the certificate resolver itself (`acme-tls/1`
ALPN + SNI → a per-domain challenge certificate with the critical acmeIdentifier extension), http-01 by a plain
listener that also 308-redirects everything else to https, and the issued chain is hot-swapped into the acceptor.
A fake ACME directory in `tests/acme_flow.rs` drives the full flow (account, order, challenge validation with the
real digest, CSR, issuance, swap) in under a second; the live QUIC endpoint still keeps the certificate it started
with (known gap, same as manual rotation).

Matrix after round 4 (unchanged within noise from round 3, as expected — none of this touches the request path):

| mode / protocol | workers | conns | nginx req/s · CPU µs/req (user) | AegisX req/s · CPU µs/req (user) | AegisX / nginx |
|---|--:|--:|---|---|--:|
| proxy, h1 | 1 | 128 | 109 338 · 9.1 (3.3) | 82 310 · 12.2 (6.4) | 75 % |
| proxy, h1 | 1 | 512 | 97 648 · 10.2 (3.9) | 72 237 · 13.8 (7.6) | 74 % |
| proxy, h1 | 2 | 128 | 149 176 · 13.1 (4.3) | 112 434 · 17.8 (8.6) | 75 % |
| proxy, h1 | 2 | 512 | 133 653 · 14.9 (5.0) | 106 817 · 18.7 (9.0) | 80 % |
| static, h1 | 2 | 128 | 265 388 · 7.5 (3.2) | 271 269 · 7.4 (3.5) | 102 % |
| static, h1 | 2 | 512 | 252 570 · 7.9 (3.1) | 249 420 · 8.0 (3.7) | 99 % |
| cache hit, h1 | 2 | 128 | 202 329 · 9.9 (3.7) | 275 775 · 7.2 (3.4) | 136 % |
| cache hit, h1 | 2 | 512 | 190 197 · 10.5 (4.0) | 253 632 · 7.9 (3.6) | 133 % |
| proxy, h1 over TLS | 2 | 128 | 112 633 · 17.7 (8.5) | 98 026 · 20.3 (10.7) | 87 % |
| proxy, h1 over TLS | 2 | 512 | 98 148 · 20.2 (9.6) | 93 181 · 21.2 (11.1) | 95 % |
| proxy, h2 | 2 | 128 | 106 909 · 18.7 (9.9) | 103 862 · 19.2 (13.1) | 97 % |
| proxy, h2 | 2 | 512 | 81 197 · 24.4 (11.5) | 74 641 · 26.7 (17.9) | 92 % |

## AWS, round 5 — cache revalidation, gunzip, autoindex, TCP streams (2026-10-04)

Shipped: conditional revalidation of stale cache entries (the refresher sends `If-None-Match` /
`If-Modified-Since` from the stored entry; a 304 renews the entry in place and answers `X-Cache: REVALIDATED`
without moving the body again), `gunzip` (route or `compression.gunzip`: upstream gzip is decoded in a streaming
`Body::Decoded` for clients that do not accept gzip, ETag weakened, Content-Length dropped), `autoindex`
(HTML directory listings when a route has no index file, directories first, names escaped and percent-encoded),
and TCP streams (`add_stream { name, listen, upstream, idle_ms }`: one SO_REUSEPORT listener per worker, the
same balancer and health state as HTTP pools, plain TCP or unix backends, per-direction idle timeout, half-close
honoured). Not built, on purpose: a disk tier for the cache, UDP streams, request-time `map` variables — the Lua
config computes static maps at load time, and the honest-limits list below carries the rest.

Final matrix (`aws-r5-*`, same method, 182 tests green):

| mode / protocol | workers | conns | nginx req/s · CPU µs/req (user) | AegisX req/s · CPU µs/req (user) | AegisX / nginx | AegisX p99 / PSS |
|---|--:|--:|---|---|--:|---|
| proxy, h1 | 1 | 128 | 110 543 · 9.0 (3.4) | 82 104 · 12.2 (6.9) | 74 % | 1.8 ms / 33 MB |
| proxy, h1 | 1 | 512 | 101 197 · 9.9 (4.0) | 74 489 · 13.4 (7.8) | 74 % | 8.2 ms / 61 MB |
| proxy, h1 | 2 | 128 | 148 718 · 13.4 (4.5) | 110 655 · 18.1 (8.9) | 74 % | 2.7 ms / 34 MB |
| proxy, h1 | 2 | 512 | 139 338 · 14.3 (4.7) | 102 546 · 19.4 (9.3) | 74 % | 10.0 ms / 67 MB |
| static, h1 | 2 | 128 | 266 792 · 7.3 (3.1) | 268 201 · 7.4 (3.5) | 101 % | 1.0 ms / 31 MB |
| static, h1 | 2 | 512 | 252 399 · 7.9 (3.2) | 249 070 · 8.0 (3.8) | 99 % | 3.4 ms / 42 MB |
| cache hit, h1 | 2 | 128 | 198 369 · 10.0 (3.9) | 277 313 · 7.1 (3.3) | 140 % | 0.9 ms / 33 MB |
| cache hit, h1 | 2 | 512 | 189 816 · 10.5 (3.8) | 255 575 · 7.8 (3.6) | 135 % | 3.7 ms / 44 MB |
| proxy, h1 over TLS | 2 | 128 | 107 860 · 18.5 (8.9) | 100 449 · 19.9 (10.5) | 93 % | 3.1 ms / 42 MB |
| proxy, h1 over TLS | 2 | 512 | 100 181 · 19.8 (9.4) | 95 356 · 20.7 (10.8) | 95 % | 9.1 ms / 75 MB |
| proxy, h2 | 2 | 128 | 106 513 · 18.8 (9.9) | 97 477 · 20.3 (13.8) | 92 % | 41 ms / 103 MB |
| proxy, h2 | 2 | 512 | 81 097 · 24.7 (11.9) | 73 109 · 27.3 (18.3) | 90 % | 156 ms / 223 MB |

Where the owner's target of 95–100 % of nginx stands: met or beaten for static files, cache hits and TLS at 512
connections; 92–93 % for h2 and TLS at 128 connections; 74 % for plain HTTP/1 proxying, where the remaining
cost sits in hyper's client-side dispatcher (round-0 and round-2 profiles) and the decision between a documented
httparse upstream client and hyper everywhere is still his.

## Second session, round 1 — the hyper proxy path from 70 % to 80–87 % of nginx (2026-10-04)

New cluster, same shape (`c7i-flex.large` × 3 + an `m7i-flex.large` build node), 10 s × 4 interleaved trials,
nginx 1.24. Engine rule in force: hyper on both sides, nothing hand-written below it. Three controls rotate with
every run: plain nginx, `nginx-id` (nginx forwarding `X-Forwarded-For/Proto` and `X-Request-Id` like our
defaults) and the lab (`bench/hyper`: a bare hyper server + hyper client, no features).

### Plain HTTP/1 proxy

| workers / conns | nginx | nginx-id | lab (bare hyper) | AegisX defaults | AegisX, identity + telemetry off |
|---|--:|--:|--:|--:|--:|
| 1 / 128 | 115 319 | 98 985 | 103 824 (90 %) | 92 739 (80.4 %) | 99 456 (86.2 %) |
| 1 / 512 | 100 791 | 91 624 | 95 748 (95 %) | 81 790 (81.1 %) | 82 868 (82.2 %) |
| 2 / 128 | 156 387 | 131 306 | 145 293 (93 %) | 128 066 (81.9 %) | 130 777 (83.6 %) |
| 2 / 512 | 136 891 | 120 536 | 129 136 (94 %) | 117 250 (85.7 %) | 119 586 (87.4 %) |

Requests per second, percentages against plain nginx (`r1l-*`). Against nginx doing the same identity work the
default build is at 93.7 % / 89.3 % / 97.5 % / 97.3 %. The session started at 70.6 % / 71.7 % / 74.9 % / 77.0 %
(`base-proxy*`, the inline-polled upstream). The lab moves between 90 % and 96 % across runs: that is the ceiling
for anything built on hyper's two-task client model on this hardware.

### Other modes (2 workers, `r1m-*`)

| mode | 128 conns | 512 conns |
|---|--:|--:|
| static files | 280 528 vs 266 270 (105 %) | 263 550 vs 254 172 (104 %) |
| proxy cache hits | 288 615 vs 199 790 (144 %) | 262 754 vs 188 164 (140 %) |
| HTTP/1 over TLS, proxied | 113 887 vs 111 610 (102 %) | 103 916 vs 103 576 (100 %) |
| HTTP/2 over TLS, proxied (10 streams) | 108 416 vs 110 616 (98 %) | 84 865 vs 91 924 (92 %) |

### What moved the number

| step | what changed | effect (1 worker, 128 conns) |
|---|---|---|
| v2 | upstream on hyper's intended model: one spawned task per upstream connection, `SendRequest` pool, non-blocking `peek` of the first body frame so small bodies return the connection at once | 80k → 85k |
| v5 | the service future is boxed per request instead of being copied into hyper's per-connection slot | +2.5 % (+5.6 % at 512) |
| v7 | request boxed end to end, head edited in place, borrowed extras | future 5.9 KB → 2.7 KB, +3 % |
| v8 | request shadow instead of a held copy; deadline reaper instead of a `Sleep` per connection; staged gateway with one compact `Flight` | +2.4 % defaults, +4.4 % lean |
| v9–v11 | response landing in sync stages, slot-indexed pool, no stored target, `Context` hot fields first | future 2.7 KB → 1.3 KB, inside noise |

### Why the rest is still there

Probe builds (a throw-away copy of the tree that short-circuits `Handler::handle` at chosen depths) gave the
cost of each layer in CPU per request: lab 9.0 µs → bare `Client::exchange` 9.3–9.6 → + route lookup and
`Proxy::forward` in a small future 9.7 → the full staged pipeline 10.2 → defaults 10.7. Every stage after the
route lookup (`guard`, `shape`, `aim`, `choose`, `land`, `complete`) measures as free; the step from 9.7 to 10.2
is reproduced exactly by padding the small future with 704 bytes that are written before the upstream wait and
read after it (103.1k → 98.1k). On this hardware a cache line of per-request state touched across the upstream
wait costs ≈ 45 ns, about 0.45 % of throughput: with 128 requests in flight the state has left the cache by the
time the response arrives. The instances expose no hardware counters, so the evidence is these differential
runs, not miss counts.

Three findings came out of that:

- Holding a copy of the request for a possible retry kept its header `Bytes` alive, which pins hyper's 8 KiB
  server read buffer: hyper then allocates a new buffer for every keep-alive request and buffers stop being
  recycled hot. The shadow (head packed into a per-worker arena, rebuilt only when a retry happens) removed that;
  it is worth 2 % at 128 connections and 6 % at 512 with defaults.
- rustc keeps every local that was ever borrowed in the coroutine state until its scope ends, even after it is
  moved. Sync stage functions are the only reliable way to keep request-phase temporaries out of the future.
- The remaining distance to the lab is state that the features need at response time plus one `Rc<Context>`
  line per connection; shrinking the future from 1.6 KB to 1.3 KB no longer moves the number.

Measured and rejected: PGO (+0–4 %, inside noise), `SO_INCOMING_CPU` steering (all connections land on one
worker on these instances: 112k vs 146k at 2 workers), software prefetch of the waiting state (the intrinsic
cannot be called without `unsafe` on 1.98; `core::hint::prefetch_read` is unstable). Worker pinning is worth
+4 % at 512 connections in the lab and nothing at 128; it stays opt-in (`runtime.pin`).

### Also shipped in this round

`add_listen` (extra listeners, `redirect = true` for an http → https hop that also serves ACME http-01),
`set_acl` and route `acl` (allow/deny networks, right-most untrusted forwarded hop when the peer is trusted),
route `respond` (fixed answers without an upstream), the HTTP/3 startup race fix (every SO_REUSEPORT UDP socket
is bound before the first worker starts). Found by reading the retry path, not reproduced by a test: a request
that hyper handed back unsent could reach the next backend with its mount prefix stripped twice; `exchange` now
restores the original URI before a request leaves it. 193 tests in 45 binaries, clippy `-D warnings` clean.

## Second session, round 2 — operator features on the new pipeline (2026-10-04/05)

Build v12, 201 tests in 48 binaries.

| feature | Lua | notes |
|---|---|---|
| header rules | `set_headers("response", { ["-server"] = "", ["+vary"] = "origin", ["?cache-control"] = "no-store" })`, same on routes | `-` removes, `+` appends, `?` sets only when absent, no prefix sets; one `Header::apply` for upstream requests, proxied responses and local responses; variables only with a plain set; `-host` on requests is rejected |
| PROXY protocol | `set_server { proxy_protocol = true }`, `add_listen { address = ..., proxy_protocol = true }`, `add_stream { ..., proxy_protocol = "v1" }` | v1 and v2 through the `ppp` crate; the header is mandatory on a listener that asks for it and the announced source becomes the session address; TCP streams send it to the backend |
| token bucket | `set_limits { rate_per_second = 200, rate_burst = 50 }`, same pair per route | one atomic per actor (or per actor and route), 429 with `Retry-After`; the 10-second window limit stays |
| precompressed files | `set_files { precompressed = true }` | `file.br` / `.zst` / `.gz` siblings are cached with the file and negotiated by `Accept-Encoding`, own ETag, `Vary`, never for range requests |

Measured with defaults against v11 (`r2b-*`, 3 trials): −3.4 % / −0.3 % at 1 worker (128 / 512 connections),
+1.4 % / +2.0 % at 2 workers — no change outside noise. Matrix at 2 workers (`r2m-*`): static 103 % / 102 %,
cache hits 144 % / 137 %, HTTP/1 over TLS 104 % / 103 %, HTTP/2 100 % / 93 %. Worker pinning on AegisX itself:
+0.9 % / +0.8 % at 2 workers (`r2a-2w`), inside noise, so `runtime.pin` stays off by default.

## Second session, round 3 — last round: balancing, mirroring, upstream mTLS, tracing, final matrix (2026-10-05)

Final tree = build v14, 208 tests in 50 binaries, clippy `-D warnings` clean, shellcheck clean on `bench/aws/*.sh`.
The matrix below was measured on v13; v14 only adds the listener-scope fix described under "Corrections" and has
the same request path (`r3w-1w`: 94.7k vs 94.2k at 128 connections, 82.8k vs 79.7k at 512).

| feature | Lua | notes |
|---|---|---|
| power of two choices | `set_balancer("pool", "random")` | two random backends, the one with the lower `(active + 1) / weight` wins; the pool counts leases |
| request mirroring | `add_route { ..., mirror = "shadow-pool" }` | replayable requests (no body or a buffered body) are copied to the other pool in a detached task, its answer is dropped, its failures never reach the client; the pool name is validated |
| upstream mutual TLS | `add_upstream("pool", { address = ..., tls = true, cert = "client.pem", key = "client.key" })` | client certificate per backend; `cert` and `key` must come together and need `tls` |
| access log floor | `set_access_log { min_status = 400 }` | only responses at or above the status are written |
| trace context | `set_identity { traceparent = true }` | starts a W3C `traceparent` when the request has none, forwards an existing one untouched |
| knob removed | `runtime.event_interval` | measured as noise in the lab and never changed a row here; the runtime uses tokio's default |

### Final matrix (`r3-*`, 4 trials for the proxy rows, 3 for the others)

| HTTP/1 proxy: workers / conns | nginx | nginx-id | lab (bare hyper) | AegisX defaults | AegisX, identity + telemetry off |
|---|--:|--:|--:|--:|--:|
| 1 / 128 | 114 690 | 100 612 | 108 729 (95 %) | 94 622 (82.5 %) | 93 452 (81.5 %) |
| 1 / 512 | 99 639 | 89 797 | 92 984 (93 %) | 81 554 (81.8 %) | 86 428 (86.7 %) |
| 2 / 128 | 152 941 | 131 113 | 140 268 (92 %) | 128 095 (83.8 %) | 133 400 (87.2 %) |
| 2 / 512 | 140 873 | 123 025 | 128 579 (91 %) | 115 268 (81.8 %) | 118 969 (84.5 %) |

Against nginx doing the same identity work the default build is at 94.0 % / 90.8 % / 97.7 % / 93.7 %.

| mode (2 workers) | 128 conns | 512 conns |
|---|--:|--:|
| static files | 278 562 vs 266 416 (105 %) | 264 520 vs 260 768 (101 %) |
| proxy cache hits | 284 524 vs 194 972 (146 %) | 266 028 vs 186 824 (142 %) |
| HTTP/1 over TLS, proxied | 117 575 vs 107 598 (109 %) | 102 642 vs 97 932 (105 %) |
| HTTP/2 over TLS, proxied (10 streams) | 109 633 vs 104 804 (105 %) | 83 725 vs 86 725 (97 %) |

Reading the numbers honestly:

- The 95 % target is met in every mode except plain HTTP/1 proxying, which ends at 82–84 % with defaults and
  82–87 % with the identity headers and telemetry off. The bare hyper control stops at 91–95 % on the same
  hardware, so no hyper-based proxy reaches 95 % here; the rest of the distance is the per-request state
  measured in round 1.
- Rows carry about ±3 %, and position in the rotation matters: with v14 right after nginx it measured 5.2 % /
  5.8 % below v13 (`r3v-1w`), with the roles swapped v13 measured 0.6 % / 3.8 % below v14 (`r3w-1w`). Small
  differences between neighbouring builds mean nothing. HTTP/2 at 512 connections moved between 92 % and 97 %
  across the three matrices of this session, so that row sits on the threshold, not above it.
- The HTTP/2 row at 128 connections has a 1.03 s p99 on the AegisX side in this run; in the round-1 run the
  same outlier was on the nginx side (1.07 s). Its cause was not investigated.
- HTTP/3 still has no benchmark on this cluster (no QUIC-capable load tool on the load node).

### Corrections

- Round 2 applied `server.proxy_protocol` to every listener built from the server settings, which included the
  control API and the redirect listeners. Found while reviewing the construction sites, fixed by passing the
  flag per listener (`Config::server_settings(proxy_protocol)`), covered by a test in `tests/control.rs`.

## Third session — final grand round: a real backend, resilience drills, production gaps (2026-10-05)

Builds v15–v25 on the same cluster. Mandate: stay on hyper, test against a real application, verify balancing,
upstream handling and request journeys under failure, close the feature gaps against current nginx (1.31.6
mainline, next to Ubuntu's 1.24) and Caddy (2.11.7), and say plainly what is and is not production-ready.
Final tree = v25: 224 integration tests in 51 binaries, clippy `-D warnings` clean, shellcheck clean, no `unsafe`,
no `unwrap`/`expect` in `src`. The synthetic matrix below was measured on v24; v25 adds the review fixes listed
under "Found by review" and measures the same (`ab-v25`, 2 workers: 121.6k vs 122.5k at 128 connections,
115.1k vs 110.3k at 512).

### The real workload

`bench/app` is an actix-web 4 + sqlx service on Postgres 16 (backend node, 2 workers): four tenants, 800 users,
10 240 catalog items, favorites, orders in a transaction with stock decrement, comments, HMAC bearer tokens, a
streamed upload with SHA-256, SSE, a 1 MiB blob, slow and flaky endpoints, request-id echo and `Server-Timing`.
`bench/load/mix.lua` drives it: 55 % catalog pages, 25 % item detail, 8 % favorites, 4 % favorite writes, 4 %
orders, 2 % comments, 2 % logins. Two proxy workers, 10 s × 3 interleaved trials (`real2-*`, AegisX v17).

| row | nginx 1.24 | nginx 1.31.6 | Caddy 2.11.7 | AegisX |
|---|--:|--:|--:|--:|
| mix, 64 conns: req/s (p50 / p99 ms) | 1 730 (37.6 / 60.2) | 1 676 (38.8 / 63.1) | 1 724 (38.0 / 60.9) | 1 764 (36.8 / 60.5) |
| mix, 256 conns: req/s (p50 / p99 ms) | 1 684 (164.5 / 190.1) | 1 709 (162.7 / 189.9) | 1 657 (167.4 / 195.1) | 1 678 (165.3 / 191.1) |
| mix: proxy CPU per request, µs (64 / 256) | 25.4 / 29.1 | 25.6 / 29.5 | 148.4 / 162.1 | 30.5 / 34.5 |
| 1 MiB downloads, 32 conns: req/s | 1 472 | 1 472 | 1 472 | 1 471 |
| 1 MiB downloads: proxy CPU per request, µs | 1 241 | 1 221 | 1 149 | 1 167 |
| 256 KiB uploads, 32 conns: req/s (p99 ms) | 5 294 (15.5) | 5 341 (15.4) | 5 282 (13.2) | 5 288 (11.1) |
| 256 KiB uploads: proxy CPU per request, µs | 168 | 168 | 311 | 166 |

The application and the NIC decide these rows; no proxy is visible in throughput or latency. AegisX spends 20 %
more CPU per request than nginx on the mix (the proxy sits at 5 % of its two cores there) and about the same on
bodies. With the production-style config `bench/configs/app.lua` (micro-cache keyed by `x-tenant` with a stale
window, compression, health checks, least-time balancing, header rules, per-route limits) the same mix runs at
11 564 req/s (p50 6.5 ms) at 64 connections and 9 531 at 256.

Micro-cache on the mix, every target measured alone and warm (`real4-*`, v24):

| row | nginx 1.24 | nginx 1.31.6 | AegisX |
|---|--:|--:|--:|
| origin headers honoured, 64 conns: req/s | 22 838 | 21 036 | 21 245 |
| origin headers honoured, 256 conns: req/s | 19 703 | 20 011 | 15 899 |
| forced 60 s lifetime, 64 conns: req/s (CPU µs/req) | 29 083 (17.9) | 29 243 (16.9) | 29 293 (15.3) |
| forced 60 s lifetime, 256 conns: req/s (CPU µs/req) | 27 801 (18.6) | 28 183 (19.0) | 27 965 (15.9) |

With a forced lifetime the three are identical and AegisX uses the least CPU. With the origin's `max-age=5` on
the list endpoint AegisX is at 93 % / 81 % of nginx 1.24. `bench/hits.py` counted the reason over 20 s at 64
connections: nginx sent 4 554 cacheable requests to the origin, AegisX 6 192. Two causes: nginx keeps cache time
in whole seconds, so a 5 s lifetime lasts 5–6 s there and exactly 5.000 s here; and AegisX drops an expired
entry on the first lookup, so every concurrent request for a hot key goes to the origin until the first refill
lands. With a stale window (`stale_ms`, as in `app.lua`) one request refreshes and the others are served stale;
without it there is no request coalescing (see limits).

Two harness faults surfaced on the way and are fixed: the nginx cache config ignored `Cache-Control` and cached
private answers under a shared key while AegisX honoured the origin (first run: 55.6k vs 20.9k req/s, not a
proxy difference) — `compare.py` now has `--cache-headers honor|ignore` and adds
`proxy_no_cache $http_authorization` in real mode; and the two nginx builds shared one cache directory, which
made 1.31.6 look 12–31 % faster than 1.24 (each inherited the other's refreshes) — one directory per target now.

### Resilience drill (`bench/drill.py`, two app instances behind one pool, v24: `drill3`)

| scenario | result |
|---|---|
| spread, mix at 64 conns, 8 s per policy | round_robin 49.8 / 50.2, least_conn 49.4 / 50.6, least_time 50.1 / 49.9, random 50.8 / 49.2; 0 failures; policy switched by reload |
| one backend loaded directly (shared database) | round_robin 50.4 / 49.6, least_conn 74.8 / 25.2, least_time 77.4 / 22.6 away from the loaded one; throughput unchanged because both instances share Postgres |
| `kill -9` of one backend under load, restart 7 s later | 28 709 requests, 0 client errors; 8 requests hit the dying backend and were retried; marked down and up again within one polling step (≤ 100 ms) |
| both backends killed | 20 of 20 answered 503 in ≤ 0.5 ms, first 200 six ms after the restart command returned |
| route timeout 500 ms on a 1.5 s endpoint | 504 after 591 ms (reaper tick), journey `received → forwarded → upstream_failed → failed` |
| 30 % flaky endpoint | pass-through 29.7 % errors; with `retry_on 5xx` 26.7 % → retried on the other backend when it is healthy, never worse than pass-through |
| order POST journey | 201 in 4.5 ms; `received → forwarded(backend, attempt) → completed(status, backend, attempts)` |
| SSE, 5 events 200 ms apart | first byte after 1 ms, 806 ms total |
| 5 reloads under load | 14 063 requests, 0 errors, 5 config versions |
| SIGTERM under load | 0 non-2xx, 0 read errors, exit after 114 ms |
| binary upgrade under load (second process on the same port, SIGTERM to the first) | 14 358 requests, 0 errors of any kind, old process gone after 164 ms |
| soak, 90 s at 128 conns | 128 509 requests, 0 errors, RSS 48.5–50.5 MB flat, 24 descriptors after idle |

What the drill found, in the order it found it:

- **Shutdown cut busy connections.** `serve` only waited `drain_ms` and then dropped everything: 96 read errors
  per SIGTERM at 64 connections, always the full 5 s. Now two-phase (`http/server/graceful.rs`): requests that
  arrive after the stop signal are answered with `Connection: close`, HTTP/2 gets GOAWAY at once, hyper's
  `graceful_shutdown` runs on what is left after 1 s, `drain_ms` stays the hard limit. The listener is closed
  before draining (after emptying its accept queue); it used to stay open, and under SO_REUSEPORT the kernel kept
  handing half of the reconnects to the dying process (71 → 38 → 0 errors across v18, v20, v24).
- **A quarantined pool turned partial failure into total failure.** After the blackout scenario one backend was
  still in its cool-down; the other answered 30 % 502; `retry_on 5xx` then quarantined it too and 264 of 300
  requests got 503. Now a status retry happens only when another available backend exists (otherwise the
  upstream answer is passed through and nothing is counted), and when every candidate is merely quarantined —
  not probed down, not marked `down` — the picker uses them anyway. nginx behaves the same way for a single
  server and when all servers have failed.
- **Quarantine log spam**: every third failed request logged the same line; `Backend::fail` reports only the
  transition and never shortens an existing quarantine.

The drill was repeated on v25 (`drill4`): kill under load 28 638 requests / 0 errors, blackout 20 × 503 and back
after 7 ms, 5 reloads 0 errors, SIGTERM 0 read errors in 114 ms, binary upgrade 0 errors, 60 s soak flat at 50 MB.

### Found by review

A second reader went through the day's changes. Confirmed and fixed in v25, each with a test:

- Connections aborted by the idle sweeper never gave their slot back: every stop waited the full `drain_ms`
  after the first swept connection, and with the new `max_connections` the worker would end up refusing
  everything. The count is now released by a guard that drops with the task.
- HTTP/2 and HTTP/3 requests carry the host in `:authority`; nothing copied it into `Host`, so host-scoped
  routes, `$host` and the cache key ignored it for every h2 client. `Handler::handle` sets `Host` from the
  authority. This predates the session.
- A stale answer served because the origin failed (`stale_if_error`) was written back as fresh, hiding the
  origin for another lifetime. Rescue now clears the pending fill, comes after the retry decision, and no longer
  clears the backend's failure count.
- Extra listeners were not awaited at shutdown; the accept-queue flush at stop is bounded; a `rate_key` with an
  empty value falls back to the peer instead of sharing one bucket; a pool-wide limit with `rate_key` used one
  bucket per route; cache lifetimes saturate instead of wrapping.

Open from the same review: a client that rotates the `rate_key` value gets a fresh bucket per value (as with
nginx `limit_req_zone $http_x_api_key`; pair it with a per-peer limit); transport errors on the last attempt
do not count toward quarantine; `max_connections` is enforced per worker and listener; upgraded tunnels end
when the worker has drained its HTTP connections; a response whose `Vary` set keeps changing is never stored.

### Features added

| feature | Lua | notes |
|---|---|---|
| per-backend counters | — | `aegisx_backend_{responses,failures,retries}_total{pool,backend}` and the same fields in `/state`; one padded counter block per worker, carried across reloads |
| rate limit by key | `set_limits { rate_key = "header:x-api-key" }`, same on routes (`cookie:`, `query:`, `uri`, `ip`) | HMAC of the value picks the bucket; absent or empty value falls back to the peer |
| cache `Vary` | automatic | a marker under the primary key names the varying headers, variants live under `key ∖0 epoch values`; `Vary: *` is never stored; purging the key orphans every variant |
| forced cache lifetimes | `set_cache { ignore_headers = { "cache-control", "expires", "set-cookie", "vary" } }` | `Set-Cookie` is dropped from what is stored |
| Lua standard library and includes | `ipairs`, `pairs`, `string`, `table`, `math`, `tostring`, …; `include("pools.lua")`, `include("conf.d")` | the sandbox had no library at all and a 10 000-instruction budget — a loop over 400 routes failed; budgets are now 1 MiB source per file, 64 MiB, 20 M instructions; `load`, `require`, `io`, `os` stay unavailable |
| graceful drain and zero-downtime upgrade | `set_server { drain_ms = 5000 }` | see above |
| connection cap | `set_server { max_connections = 100000 }` | excess connections are closed at accept |
| open file limit | automatic | soft `RLIMIT_NOFILE` raised to the hard limit at boot (`rlimit` 0.11), warning when it is below two descriptors per `max_in_flight` |
| systemd unit | `server/deploy/aegisx.service` | `Type=notify`, reload by SIGHUP, hardening flags |
| upstream fields in access logs | `$upstream_response_time` (alias `$upstream_header_time`), `$upstream_status`, `$upstream_attempts`; JSON `upstream_time_us`, `attempts` | they used to print `-`; the time runs from request arrival to the upstream response head |
| keep-alive request budget | `set_server { keepalive_requests = 0 }` is the default now | 1 000 (nginx's default) cost 3.5 % and p99 2.7 vs 1.9 ms at 2 workers / 128 connections under wrk (`ka-2w`); nginx in the harness has always run with 100 000 |

From the earlier part of the session (v17): send timeout for slow readers, forward-auth identity headers
stripped from client requests, `underscores_in_headers`, `keepalive_time_ms`, `least_time` balancing, cache
`key_headers` and `stale_if_error`, SNI routing for TCP streams, post-quantum key exchange preference,
`sd_notify`.

### Final synthetic matrix (`final-*`, v24, 4 trials for the proxy rows, 3 for the others)

| HTTP/1 proxy: workers / conns | nginx 1.24 | nginx 1.31.6 | nginx-id | lab (bare hyper) | AegisX defaults | AegisX, identity + telemetry off |
|---|--:|--:|--:|--:|--:|--:|
| 1 / 128 | 107 410 | 110 672 | 98 608 | 108 650 (101 %) | 92 308 (85.9 %) | 95 729 (89.1 %) |
| 1 / 512 | 98 630 | 98 547 | 90 532 | 88 124 (89 %) | 79 973 (81.1 %) | 84 573 (85.7 %) |
| 2 / 128 | 151 481 | 151 115 | 131 137 | 143 094 (94 %) | 121 330 (80.1 %) | 128 009 (84.5 %) |
| 2 / 512 | 140 620 | 141 114 | 124 791 | 129 069 (92 %) | 112 979 (80.3 %) | 116 610 (82.9 %) |

Percentages against nginx 1.24. Against nginx doing the same identity work: 93.6 % / 88.3 % / 92.5 % / 90.5 %.

| mode (2 workers) | 128 conns | 512 conns |
|---|--:|--:|
| static files | 273 485 vs 267 733 (102 %) | 252 268 vs 253 198 (100 %) |
| proxy cache hits | 284 591 vs 195 618 (145 %) | 256 029 vs 187 638 (136 %) |
| HTTP/1 over TLS, proxied | 109 974 vs 105 653 (104 %) | 101 982 vs 102 197 (100 %) |
| HTTP/2 over TLS, proxied (10 streams) | 108 456 vs 108 154 (100 %) | 81 343 vs 87 736 (93 %) |

nginx 1.31.6 is within 4 % of 1.24 on every row. Reading the numbers:

- The 90 % target is met everywhere except plain HTTP/1 pass-through, which stays at 80–86 % with defaults and
  83–89 % lean; against nginx doing the same header work it is 88–94 %. The bare hyper control measured 89–101 %
  in this run.
- This round's attempts on that row, all measured: software prefetch of the waiting request state (v15, no
  change), a LIFO pool of request-future boxes (v16: +4–5 % at 1 worker / 512, −9 % at 2 workers / 128, removed),
  transparent huge pages through mimalloc (`thp-*`, inside noise for AegisX and the lab), the keep-alive budget
  (above, +3.5 %). A fresh profile of v20 is flat: AegisX's own frames are about 9 % of the samples, no symbol
  above 3 %; hyper's per-response body channel is 2.9 %, clock reads 1.7 %. There is no single thing left to
  remove, and a hyper-based proxy with these features does not reach 90 % of bare nginx here.
- HTTP/2 at 512 connections (5 120 concurrent streams): up to 0.06 % of the requests are answered 503 by the
  default `max_in_flight = 4096`. The same happened in the earlier matrices (35–372 per trial) and was not
  reported there.

### Corrections

- `aegisx-v19` was byte-identical to v18: `sync` kept laptop mtimes, files edited while a remote build ran
  arrived older than that build, and cargo called the release profile fresh. `sync` now copies by checksum
  without times. No number in this report comes from v19.
- The first real cache row (`real2-cache`, `real3-*`) is superseded by `real4-*` for the two harness faults
  described above.

## Fourth session — feature completion: the remaining nginx and Caddy list (2026-10-05)

Build v31 (v30 for the matrix): 237 integration tests in 51 binaries, clippy `-D warnings` clean, shellcheck clean, no `unsafe`. Order:
implement the fourteen items left after the third session, keep the request path as fast as it was, keep the
configuration optional (every new knob has a default; the binary still starts with no file at all).

### What was added

| item | Lua | notes |
|---|---|---|
| cache request coalescing | automatic (`set_cache { lock = true, lock_ms = 1000 }`) | the first miss for a key claims it; followers wait for that fill (bounded by `lock_ms`) and are served from the cache; an answer that could not be stored leaves a 5 s pass marker so an uncacheable URL does not queue its followers |
| disk cache tier | `set_cache { path = "/var/cache/aegisx", disk_bytes = 1073741824 }` | `core/cache::Shelf`: one file per entry, in-memory index, least-recently-used eviction; memory stays the first tier, disk entries are thawed on a hit and survive a restart; `Vary` markers are persisted, purge removes both tiers |
| FastCGI | `add_upstream("php", { address = "unix:/run/php/php-fpm.sock", protocol = "fastcgi" })`, `add_route { path = "/", upstream = "php", root = "/srv/app/public" }` | own record client in `http/fastcgi`; CGI parameters, front controller `index.php`, `/x.php/extra` split into script and `PATH_INFO`; existing files are served statically for GET/HEAD, `.php` is never served as a file; the response body streams; one connection per request, request bodies are buffered |
| ACME DNS-01 | `set_tls { acme = { domains = { "*.example.com" }, challenge = "dns_01", dns_hook = "/etc/aegisx/dns-hook", dns_wait_ms = 30000 } }` | the hook is called as `hook add|remove <record> <value>`, so any DNS provider works; wildcards are accepted only with this challenge |
| request-time variables | `add_map("tier", { from = "header:x-plan", values = { pro = "gold", ["~^ent"] = "platinum" }, default = "free" })`, `add_geo("zone", { values = { ["10.0.0.0/8"] = "office" }, default = "world" })`, `add_split("lane", { from = "cookie:uid", buckets = { canary = 5, stable = 95 } })` | usable as `$tier` in header templates and in `match_vars`; `geo` uses the client address behind trusted peers |
| matchers | `match_query = { debug = "1" }`, `match_vars = { lane = "canary" }`, values `"exact"`, `"~regex"`, `"*"` (also for `match_headers`) | compiled once per route |
| dynamic upstreams | `POST /api/v1/upstreams {"pool":"api","backend":{"address":"10.0.0.7:8080","weight":2}}`, `DELETE /api/v1/upstreams {"pool":"api","address":"10.0.0.7:8080"}`, `GET /api/v1/upstreams` | goes through the same validation and swap as a reload; counters and health carry over; a file reload replaces what the API changed |
| JWT | `add_jwt("main", { secret = env("JWT_SECRET"), issuer = "…", audience = "…", claims = { sub = "x-user" } })` or `{ jwks = "keys.json" }`, then `add_route { …, jwt = "main" }` | HS256/384/512, RS256/384/512, PS256/384/512, ES256/384, EdDSA on aws-lc-rs; a verifier holds either a secret or public keys, never both; `exp`, `nbf`, `iss`, `aud` with `leeway_s`; claim headers replace whatever the client sent; verified tokens are remembered until they expire |
| OTLP export | `set_telemetry { otlp = "collector:4318", otlp_interval_ms = 5000, otlp_headers = { authorization = "…" } }` | OTLP/HTTP JSON: one span per finished journey with its stages as events, plus request counters |
| log sinks | `set_access_log { path = "syslog://10.0.0.5:514" }`, `udp://`, `tcp://`, `stdout`, or a file | RFC 5424 framing for syslog, one datagram per line |
| slow start and ejection cap | `set_balancer("api", { slow_start_ms = 30000, max_ejected = 50 })` | a backend that returns from quarantine, from a failed probe or through the API ramps from almost nothing to its weight; no more than `max_ejected` percent of a pool is quarantined; transport failures now count on every attempt |
| connection limit by key | `add_route { …, concurrency_limit = 4, rate_key = "header:x-api-key" }` | the same key that scopes rate limits scopes the in-flight limit |
| tunnel drain | automatic | upgraded connections keep the worker alive until they end or `drain_ms` passes |
| UDP streams | `add_stream { name = "dns", listen = "0.0.0.0:53", upstream = "dns", udp = true, idle_ms = 60000 }` | one upstream socket per client address, balanced like TCP streams |
| body rewriting | `add_route { …, replace = { ["http://old"] = "https://new" }, replace_types = { "text/html" } }` | streaming, matches across chunk boundaries; `Accept-Encoding` is not forwarded on such routes |
| internal routes and `X-Accel-Redirect` | `add_route { path = "/vault", root = "/srv/files", internal = true }` | clients get 404; an upstream answer carrying `X-Accel-Redirect: /vault/x` is replaced by that file, keeping `Content-Type`, `Content-Disposition`, cache and cookie headers |
| QUIC certificate rotation | automatic | HTTP/3 endpoints pick up reloaded and ACME-issued certificates within a second |
| HTTP/3 benchmark | `bench/h3load`, `compare.py --protocol h3` | closed-loop load generator on quinn + h3 |

Layering: `HashKey` / `Hint` moved from the balancer to `http/key`; variables and matchers live in
`http/variable`, the body rewriter in `http/encode`, FastCGI in `http/fastcgi`, JWT verification in `core/jwt`,
the disk shelf in `core/cache`; `Body::Boxed(Box<dyn Frames>)` is the one extension point for new body sources.
One new direct dependency in the server, `httparse` (already in the lock file through hyper); the load
generator is its own crate. 256 source files, 22.0k lines.

Not done, deliberately: on-demand TLS (it needs a per-name certificate store, issuance inside the handshake path
and an allow policy, none of which could be exercised properly here); OCSP fetching (Let's Encrypt switched its
OCSP responders off in 2025; stapling from files stays); GeoIP database lookups (`add_geo` covers networks);
the panel.

### Measured (`fin2-*`, `real5-*`, `drill5`, `ab-*`; v30 unless noted, 3 trials)

No cost on the request path: v30 against v25 at 2 workers 121.2k vs 122.2k (128 connections) and 107.5k vs 108.5k
(512); v31 against v25 at 1 worker 93.6k vs 94.7k and 79.9k vs 81.1k.

| HTTP/1 proxy: workers / conns | nginx 1.24 | nginx-id | lab (bare hyper) | AegisX defaults | AegisX, identity + telemetry off |
|---|--:|--:|--:|--:|--:|
| 1 / 128 | 106 179 | 96 630 | 107 137 (101 %) | 90 284 (85.0 %) | 94 562 (89.1 %) |
| 1 / 512 | 100 513 | 83 816 | 92 119 (92 %) | 75 330 (74.9 %) | 83 603 (83.2 %) |
| 2 / 128 | 146 286 | 125 868 | 138 984 (95 %) | 119 259 (81.5 %) | 122 467 (83.7 %) |
| 2 / 512 | 134 770 | 116 218 | 126 370 (94 %) | 112 336 (83.4 %) | 112 832 (83.7 %) |

Against nginx doing the same identity work: 93.4 % / 89.9 % / 94.7 % / 96.7 %. The 1 worker / 512 row moved
between 75 % and 81 % across this session's runs (79.9k against 101.3k in `ab-v31-1w`).

| mode (2 workers) | lower load | higher load |
|---|--:|--:|
| static files (128 / 512 conns) | 270 780 vs 264 877 (102 %) | 246 838 vs 249 161 (99 %) |
| proxy cache hits (128 / 512) | 274 036 vs 198 140 (138 %) | 259 053 vs 185 298 (140 %) |
| HTTP/1 over TLS (128 / 512) | 112 543 vs 109 305 (103 %) | 99 632 vs 99 708 (100 %) |
| HTTP/2 over TLS, 10 streams (128 / 512) | 106 740 vs 105 555 (101 %) | 78 716 vs 85 658 (92 %) |
| HTTP/3, 10 streams (16 / 64 conns, v31, against nginx 1.31.6) | 85 233 vs 129 909 (66 %) | 82 075 vs 124 364 (66 %) |

HTTP/3 has its first numbers: two thirds of nginx's QUIC stack, about seven times Caddy (12 673 and 11 110
req/s on the same rows). nginx answered 495 and 1 923 requests of those runs with errors, AegisX none. The first
attempt returned 502 for every request: a GET over HTTP/3 reached the gateway before the client's FIN, was
forwarded with `Transfer-Encoding: chunked`, and the bench backend refuses chunked requests. `http/h3` now reads
to the end of a request that has no `Content-Length` and a body-less method before handing it over (v31).

Real workload, mix through the micro-cache, every target alone and warm (`real5-*`):

| row | nginx 1.24 | nginx 1.31.6 | AegisX |
|---|--:|--:|--:|
| origin headers honoured, 64 conns: req/s (p99 ms) | 22 555 (28.2) | 23 368 (28.0) | 24 752 (20.7) |
| origin headers honoured, 256 conns: req/s (p99 ms) | 20 614 (124.0) | 20 243 (134.5) | 23 016 (86.9) |
| forced 60 s lifetime, 64 conns: req/s (CPU µs/req) | 29 014 (15.3) | 30 353 (15.1) | 30 035 (13.7) |
| forced 60 s lifetime, 256 conns: req/s (CPU µs/req) | 28 663 (16.6) | 28 041 (16.5) | 28 331 (15.1) |

Coalescing turned the one real row that trailed (93 % / 81 % in the third session) into 110 % / 112 % of
nginx 1.24. The pass-through mix stays backend-bound and identical for all four proxies (1 478–1 523 req/s);
with `configs/app.lua` it runs at 11 477 req/s at 64 connections and 11 007 at 256 (9 531 before).

Drill on v30 (`drill5`): kill under load 28 578 requests / 0 errors; blackout 20 × 503 in ≤ 0.5 ms, back after
7 ms; 5 reloads 0 errors; SIGTERM 0 read errors in 114 ms; binary upgrade 0 errors; flaky 30 % endpoint 28.7 %
errors passed through, 18.7 % with `retry_on 5xx`; 60 s soak flat at 51 MB.

## Fifth session — round 1 of the closing mandate: crates under the sensitive paths (2026-10-05)

The mandate for the last three rounds: hyper and tokio stay, no hand-written protocol logic (framing, connection
reuse, retries, ranges, FastCGI, TLS, ACME), the strongest mature crates instead, a feature matrix against nginx
and Caddy (`server/FEATURES.md`, counted by `bench/coverage.py`), then the priority features.

**Verified on the build node (clippy `-D warnings`, tests, release build v32):**

- Upstream on `hyper-util`'s legacy client: one pooled client per upstream slot, the body crossing its `Send`
  bound through `send_wrapper`, a `tower_service::Service<Uri>` connector for tcp / unix / TLS with ALPN. The
  hand-written pool, the reaper and the separate h2 client are deleted; tunnels and TCP streams run on
  `tokio::io::copy_bidirectional` with `tokio-io-timeout`. 237 tests passed on this state.
- FastCGI on `fastcgi-client` + `async-stream`: optional keep-alive (`set_balancer(name, { keepalive = n })`, off
  by default because every parked link pins one php-fpm child per worker), request bodies with a known length are
  streamed, a parked link the backend closed is replaced, a failed first attempt on a reused link is retried on a
  fresh one. 7 new tests (`tests/fastcgi.rs`) plus the existing one, three consecutive green runs. The full suite
  was not rerun after this step: the node went away first.

**Measured cost of the pooled client** (`results/aws` `ab-v31-*` / `ab-v32-*`, same job, nginx 1.24 as the control,
2-vCPU nodes, wrk 10 s × 3):

| row | v31 req/s | v32 req/s | change | v32 vs nginx |
|---|--:|--:|--:|--:|
| 1 worker, 128 conn | 90 907 | 81 954 | −9.8 % | — |
| 1 worker, 512 conn | 77 962 | 73 136 | −6.2 % | 76.9 % (v31 80.4 %) |
| 2 workers, 128 conn | 123 213 | 108 156 | −12.2 % | — |
| 2 workers, 512 conn | 113 575 | 102 345 | −9.9 % | 73.7 % (v31 82.7 %) |

User CPU rose from 5.4 to 7.1 µs per request (1 worker, 128 connections). The legacy client clones itself and the
connector per request, boxes the response future and spawns one `on_idle` task per HTTP/1 exchange; none of it is
ours to tune. This is the price of taking connection reuse from a crate, kept as measured. The composable
`hyper_util::client::pool` is the candidate to try in round 2; its types cannot be named yet.

**Written, not compiled, not run** — at 15:35 UTC every node stopped answering on port 22 from an allowed address
and the AWS access key started returning `InvalidClientTokenId`; nothing below has seen a compiler. `FEATURES.md`
carries these rows as `wip` and does not count them:

- byte ranges parsed and bounded by `http-range-header`;
- GeoIP databases: `add_geoip(name, { path, field, default })` over `maxminddb`, usable in header templates,
  `match_vars`, maps and keys;
- access-log rotation by size or by period over `file-rotate` (`rotate_bytes` | `rotate_every`, `rotate_keep`,
  `rotate_compress`);
- several rate limits at once: `rate_rules = { { key, rate, burst }, … }`, checked together and charged only when
  all pass;
- upstream API edits kept as an overlay that every reload re-applies (`POST /api/v1/upstreams/reset` forgets
  them);
- buffered uploads past `limits.spool_bytes` go to an unnamed temp file (`tempfile`);
- health checks that match the response body (the probe itself is in the verified v32; its test is not);
- a local certificate authority (`set_tls { internal = true }`, `rcgen`) that mints per-name certificates inside
  the handshake, and on-demand ACME before the handshake (`on_demand = { names, ask, capacity, per_hour }`,
  `tokio-rustls` `LazyConfigAcceptor`);
- Lua `on_request` / `on_response`, one VM per worker with instruction and memory budgets, skipped entirely when
  no hook is declared;
- presets `static_site`, `spa`, `php_app`;
- cache purge by prefix, host and tag as bans checked on lookup (`DELETE /api/v1/cache { prefix | tag, host }`,
  tags from `Cache-Tag` and `Surrogate-Key`; 256 bans, then a full purge), single ranges answered from cached
  objects, `X-Accel-Expires`, `cache_bypass` by variable;
- DNS SRV upstreams over `hickory-resolver` (`add_upstream(name, { srv = "_http._tcp.name" })`; lowest priority
  group, weights unused; no DNS fixture, so only validation and the failing lookup are tested);
- WebSocket over HTTP/2: extended CONNECT is bridged to an HTTP/1 upgrade (`enable_connect_protocol`), an
  upstream that does not switch protocols yields 502;
- small gaps: a leading `!` negates any match value (covers `valid_referers`), route `method`, `log = false`,
  `charset`, `scheme`, `abort`, `satisfy_any`, `secure_link` (HMAC-SHA256 over path and expiry),
  `set_tls { min_version }`, stream access lists and `max_connections`, `set_balancer(name, { slow_ms })` as a
  latency breaker, `add_keyval` with `GET|PUT|DELETE /api/v1/keyval/<name>`.

103 Early Hints is now `skip`: hyper 1.11 has no server call that emits an interim response, and framing stays
in hyper.

A first cut of the round-3 language is written the same way (unverified): `listen`, `upstream "name" { … }`,
`site "host" { route "/path" { … }, … }` and a host-less `route`, with `30s` / `10MB` / `600/m` units and the
short words `to`, `files`, `spa`, `respond`, `strip`, `limit`, `burst`, `timeout`, `max_body`, `allow`, `deny`,
`auth`. Every `add_route` field is still accepted inside `route`, and the `set_*` / `add_*` functions are
unchanged (`config/dsl/plain.rs`, `tests/plain.rs`).

**Counted coverage:** 116 done, 33 wip, 1 todo, 6 gap, 23 skipped → 74.4 % verified (72.8 % before the round);
95.5 % if every `wip` row passes. None of the 33 has been compiled.

**Not done in this round:** `http-cache-semantics` (the cache still reads `Cache-Control` itself), slice fetching
and caching, and six gaps: `server_name` regex, addition, upstream queue, stream TLS termination, stream maps,
`default_sni` / `strict_sni_host`. 4- and 8-worker rows still need larger instances. Rounds 2 and 3 have not
started: profiling and benchmarks need the build node and the bench trio.

**Limits that came with the crates:** `fastcgi-client` writes the whole request before it reads (an early reply
during a long upload is not seen), ends a response silently when the backend closes without END_REQUEST, and
sends two empty STDIN records for a request without a body — a FastCGI server that closes without draining
answers with a reset (php-fpm drains). On-demand ACME does not cover QUIC handshakes and has no test against a
directory; it serialises issuance through one lock.

## State of the tree (2026-10-05, after the fourth session)

The 2026-10-04 section below describes the tree after the first six rounds; since then: the upstream runs on
hyper's task model with a slot-indexed idle pool, a request shadow for retries and a deadline reaper; the gateway
is a staged pipeline over one compact `Flight`; `core` gained `arena`, `pool`, `rt::{Reaper, Fuse}`,
`net::{Nets, Preamble}`, `str`, `sync::Tally`, `sys::{notify, files}`, `jwt`, `cache::Shelf`; `http` gained `key`,
`variable`, `fastcgi`, `encode::Replaced`, `body::Frames`. Features added in the second, third and fourth
sessions: extra listeners with redirect, ACLs, fixed responses, header rules, PROXY protocol, token-bucket rate
limits with keys, keyed connection limits, precompressed static files, power-of-two-choices and least-time
balancing, slow start, ejection cap, dynamic upstreams API, request mirroring, upstream mutual TLS, access-log
floor, upstream fields and network sinks, trace context, OTLP export, cache `Vary` / key headers /
stale-if-error / forced lifetimes / request coalescing / disk tier, SNI and UDP stream routing, send timeout,
connection cap, graceful drain for connections and tunnels, zero-downtime binary upgrade, per-backend counters,
Lua library and includes, request-time variables and matchers, body rewriting, internal routes with
`X-Accel-Redirect`, FastCGI, JWT, ACME DNS-01, QUIC certificate rotation, systemd unit. Layers
`core ← http ← config ← app` with no upward import; 237 integration tests in 51 binaries.

Honest limits: plain-HTTP/1 pass-through 75–85 % of nginx with defaults (90–97 % against nginx doing the same
identity work); HTTP/3 at 66 % of nginx; 20 % more proxy CPU per request than nginx on the real mix; FastCGI
opens one connection per request and buffers request bodies; the dynamic upstreams API does not survive a file
reload; cache lifetimes are exact where nginx rounds up to a second; `rate_key` trusts the value it is given;
`max_connections` is per worker and listener; UDP replies larger than 16 KiB are truncated; timeouts on
upstream exchanges fire up to 100 ms late; mirrored requests skip streaming bodies; PROXY protocol headers
larger than 2 KiB are rejected; no on-demand TLS, OCSP fetching, GeoIP databases or request-time scripting; the
default `max_in_flight` sheds load at 5 120 concurrent HTTP/2 streams; the new panel is still not built. The
build node and the bench trio are still running at the time of writing.

## State of the tree (2026-10-04, after the first six AWS rounds)

Engine: hyper 1.11 server (h1 + h2, `auto` builder, one `LocalSet` per core, idle sweeper), hyper `client::conn`
upstream (h1 pools with the soft idle cap, h2 multiplexed sessions), quinn + h3 for HTTP/3, rustls 0.23 on
aws-lc-rs. Features at nginx/Caddy level: hosts, prefix/exact/regex/`prefer` locations, `try_files`,
`error_pages` with `intercept_errors`, rewrites and redirects (`proxy_redirect`, cookie domain/path), static
files with memory cache, conditional requests, byte ranges incl. multipart, `autoindex`; gzip/brotli/zstd and
`gunzip`; in-memory proxy cache with stale-while-revalidate, conditional revalidation and purge API; round-robin /
least-conn / ip_hash / hash / sticky, weights, `backup` / `down`, health probes, retries, named backends with
DNS refresh, unix sockets both ways; TLS with SNI, session tickets, mutual TLS variables, OCSP staple files,
ACME auto-HTTPS (tls-alpn-01 and http-01); HTTP/3 with Alt-Svc; basic and forward auth; rate limits per actor
and route, `concurrency_limit`, `bandwidth` pacing, request/response buffering, client/upstream timeouts;
access log in combined/json/custom patterns; Prometheus metrics, control API, hot reload, TCP stream proxying;
request-id, trusted peers, header templates with `$vars`; the model assistant DSL for background scoring.
182 integration tests in 41 binaries, clippy `-D warnings` clean, no `unsafe`.

Honest limits: plain-HTTP/1 proxying is 71–80 % of nginx on this hardware at this point (superseded by the
second-session round 1 below); per served connection AegisX holds ≈ 22 KB client side plus ≈ 35 KB per pooled upstream
connection against nginx's pool buffers; h2 at 5 120 concurrent streams needs a large idle pool (223 MB PSS);
HTTP/3 has no benchmark on this cluster (no QUIC-capable load tool) and live QUIC endpoints do not pick up rotated
or ACME-issued certificates until restart; OCSP responses are not fetched, only loaded from files; TLS 1.3
resumption is proven by test, not by `openssl s_time`; `$upstream_response_time` in access patterns prints `-`;
no disk cache tier, no UDP streams, no request-time `map`, no webhooks or cooperative cancellation, the new panel
still needs Node 26.10; macOS/Windows paths compile-guarded but untested. The AWS cluster (`bench/aws`) is torn
down after this report; `terraform.tfstate` stays for re-creation.
