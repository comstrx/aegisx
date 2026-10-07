# AegisX feature matrix

Coverage against nginx (open source modules, plus the commercial ones people ask for) and Caddy (standard
directives, matchers and global options). One row per module or directive.

Status: `done` works and is tested · `wip` code and tests are written but have not been compiled or run yet (the
build node has been unreachable since 2026-10-05) · `todo` missing, scheduled in the three closing rounds · `gap`
missing, not scheduled · `skip` intentionally unsupported, with the reason.

Coverage = `done` / (`done` + `wip` + `todo` + `gap`); `skip` rows are outside the count and `wip` rows do not count
as covered until they pass on the build node. The numbers at the bottom are
produced by `python3 ../bench/coverage.py FEATURES.md` and are refreshed at the end of every round.

## What rests on which crate

Sensitive protocol work is delegated; AegisX composes it.

| concern | crate | state |
|---|---|---|
| HTTP/1 and HTTP/2 framing, server and client | `hyper` | done |
| upstream connection reuse, pooling, retry of unsent requests | `hyper-util` legacy client | done (v32; measured cost against the hand-written pool: −6 % to −12 % throughput) |
| HTTP/3 and QUIC | `h3`, `h3-quinn`, `quinn` | done |
| TLS, session resumption, client certificates | `rustls`, `tokio-rustls`, `aws-lc-rs` | done |
| ACME account, orders, challenges | `instant-acme` | done |
| certificates for the internal CA and challenges | `rcgen`, `x509-parser` | done / wip (internal CA) |
| PROXY protocol v1 and v2 | `ppp` | done |
| FastCGI records, keep-alive, streaming | `fastcgi-client`, `async-stream` | done |
| byte ranges | `http-range-header` | wip |
| cache freshness, storability, `Vary`, revalidation | `http-cache-semantics` | todo (round 1) |
| compression codecs | `flate2` (zlib-rs), `brotli`, `zstd` | done |
| TCP and UDP sockets, bidirectional copy, idle timeouts | `tokio` (`copy_bidirectional`), `tokio-io-timeout`, `socket2` | done |
| DNS: A, AAAA, SRV | `hickory-resolver` (SRV), the system resolver (A, AAAA) | wip |
| GeoIP databases | `maxminddb` | wip |
| JWT signatures | `aws-lc-rs` | done |
| request parsing helpers | `httparse`, `memchr`, `regex`, `matchit` | done |
| configuration language | `mlua` (Lua 5.5) | done |
| in-memory caches | `quick_cache`, `papaya` | done |
| log rotation | `file-rotate` | wip |
| upload spooling | `tempfile` | wip |

## nginx — core and HTTP core

| item | status | AegisX |
|---|---|---|
| core: `worker_processes`, CPU affinity | done | `runtime { workers, pin }`, thread per core |
| core: `worker_rlimit_nofile` | done | raised to the hard limit at boot |
| core: `worker_connections` | done | `server { max_connections }` |
| core: `error_log` levels, JSON | done | `log { level, json }` |
| core: `include` | done | `include("file-or-dir")` |
| core: `env` | done | `env("NAME")` |
| core: `daemon`, `user`, `pid` | skip | systemd owns the process; unit file ships in `deploy/` |
| core: `load_module` (dynamic modules) | skip | one static binary; extension is Lua hooks |
| core: binary upgrade without downtime | done | second process on the same port, SIGTERM to the first |
| core: graceful reload (`SIGHUP`) | done | reload keeps connections, counters and health |
| http core: `listen` (address, unix, reuseport, backlog, proxy_protocol, ssl, quic) | done | `listen`, `add_listen`, `set_listen_unix` |
| http core: `server_name` exact and wildcard | done | route `host`, `*.example.com` |
| http core: `server_name` regex | gap | |
| http core: `location` prefix, exact, regex, `^~` | done | route `path`, `exact`, `regex`, `prefer` |
| http core: named locations, `internal` | done | `internal = true` routes |
| http core: `root`, `alias`, `index`, `try_files` | done | route `root`, `strip_prefix`, `index`, `try_files` |
| http core: `error_page`, `recursive_error_pages` | done | `error_pages`, per route too |
| http core: `client_max_body_size` | done | `limits { max_body_bytes }` |
| http core: `client_body_buffer_size`, temp files | wip | `limits.spool_bytes`, `limits.spool_dir`; buffered uploads past the threshold go to an unnamed temp file (`tempfile`) |
| http core: `client_header_timeout`, `client_body_timeout`, `send_timeout` | done | `server` and `limits` timeouts |
| http core: `keepalive_timeout`, `keepalive_requests`, `keepalive_time` | done | `server { keepalive_* }` |
| http core: `large_client_header_buffers`, `max_headers` | done | `server { buffer, max_headers }` |
| http core: `underscores_in_headers`, `merge_slashes` | done | |
| http core: `limit_except` | done | route `methods` |
| http core: `limit_rate`, `limit_rate_after` | done | `bandwidth`, `bandwidth_after` |
| http core: `satisfy any` | wip | route `satisfy_any = true`: an admitted address or a passed login, token or signed link is enough |
| http core: `etag`, `if_modified_since` | done | |
| http core: `types`, `default_type` | done | built-in MIME table |
| http core: `open_file_cache` | done | `files { cache_* }` |
| http core: `sendfile`, `aio`, `directio` | skip | files are served from the memory cache or async reads; no kernel bypass under tokio |
| http core: `resolver` | done | names are re-resolved on `client.resolve_ms` |
| http core: `absolute_redirect`, `server_name_in_redirect` | skip | generated redirects are always relative, which is nginx with `absolute_redirect off`; there is nothing to switch |
| http core: `lingering_close` | done | unread request bodies are drained |
| http core: `reset_timedout_connection` | skip | sockets are closed normally |
| http core: 103 Early Hints | skip | hyper's server has no call that emits an interim response, and framing stays in hyper |

## nginx — HTTP modules

| item | status | AegisX |
|---|---|---|
| access (`allow`, `deny`) | done | `acl`, route `acl` |
| addition (`add_before_body`, `add_after_body`) | gap | |
| auth_basic | done | route `basic_auth` |
| auth_request | done | route `forward_auth` |
| auth_jwt (Plus) | done | `add_jwt`, route `jwt` |
| autoindex | done | route `autoindex` |
| browser, empty_gif, userid | skip | legacy helpers; `respond` covers the fixed answers |
| charset | wip | route `charset = "utf-8"` is appended to textual types that carry none |
| dav | skip | WebDAV is out of scope |
| fastcgi | done | pool `protocol = "fastcgi"`; keep-alive and body streaming are todo |
| fastcgi keep-alive, request streaming | done | `fastcgi-client`; `set_balancer(name, { keepalive = n })` parks links per worker, bodies with a known length are streamed |
| flv, mp4, hls (Plus) | skip | media pseudo-streaming is out of scope; byte ranges work |
| geo | done | `add_geo` |
| geoip, geoip2 | wip | `add_geoip(name, { path, field, default })` over `maxminddb`; text and number fields, reloaded with the config |
| grpc (`grpc_pass`) | done | HTTP/2 upstreams with trailers |
| gunzip | done | route `gunzip` |
| gzip, brotli, zstd | done | `compression` |
| gzip_static and friends | done | `files { precompressed }` |
| headers (`add_header`, `expires`) | done | header rules with `+`, `-`, `?` |
| image_filter, xslt, perl | skip | content transformation belongs in the application |
| index, random_index | done | `index`; random index is skipped |
| js (njs) | wip | Lua `on_request` / `on_response`, one VM per worker, instruction and memory budgets, no cost when absent |
| keyval (Plus) | wip | `add_keyval(name, { from, default })` + `GET|PUT|DELETE /api/v1/keyval/<name>`; kept across reloads, not across restarts |
| limit_conn | done | `concurrency_limit`, keyed |
| limit_req | done | `rate_per_second`, `rate_burst`, `rate_key` |
| limits by several keys at once | wip | `rate_rules = { { key, rate, burst }, … }` on a route or in `set_limits`; checked together, charged only when all pass |
| log (`access_log`, `log_format`) | done | combined, JSON, custom pattern, syslog, udp, tcp |
| log rotation | wip | `set_access_log { rotate_bytes | rotate_every, rotate_keep, rotate_compress }` over `file-rotate` |
| map | done | `add_map` |
| memcached | skip | |
| mirror | done | route `mirror` |
| proxy (`proxy_pass` and the usual knobs) | done | |
| proxy: `proxy_next_upstream` | done | `retry_on`, `attempts` |
| proxy: `proxy_buffering`, `proxy_request_buffering` | done | `buffer_response`, `buffer_request` |
| proxy: `proxy_redirect`, cookie domain and path | done | |
| proxy: `proxy_cache` in memory | done | `cache` |
| proxy: `proxy_cache_path` on disk | done | `cache { path, disk_bytes }` |
| proxy: `proxy_cache_lock` | done | on by default |
| proxy: `proxy_cache_use_stale`, background update | done | `stale_ms`, `stale_if_error` |
| proxy: `proxy_cache_key`, `proxy_ignore_headers` | done | `key_headers`, `ignore_headers` |
| proxy: `proxy_cache_bypass`, `proxy_no_cache` by variable | wip | route `cache_bypass = { "variable" }`: a non-empty value other than `0` skips lookup and storage |
| proxy: `proxy_cache_purge` (Plus) | done | control API, by key |
| cache purge by prefix or tag | wip | `DELETE /api/v1/cache { prefix | tag, host }`; bans checked on lookup, tags from `Cache-Tag` and `Surrogate-Key` |
| proxy: `proxy_store` | skip | the disk cache covers it |
| proxy: `X-Accel-Redirect` | done | `internal` routes |
| proxy: `X-Accel-Expires`, `X-Accel-Buffering` | wip | `X-Accel-Expires` (seconds, `@time`, `0`) decides the lifetime; `X-Accel-Buffering` is not read |
| proxy_protocol | done | in and out |
| random (two choices) | done | `policy = "random"` |
| range requests, multipart ranges | done | |
| realip | done | `identity { trusted_peers }` |
| referer (`valid_referers`) | wip | `match_headers = { referer = "!~pattern" }` on a `deny` route |
| rewrite (`rewrite`, `return`, `set`, `if`) | done | `rewrite`, `respond`, redirects, variables, matchers |
| scgi, uwsgi | skip | FastCGI and HTTP cover the same applications |
| secure_link | wip | route `secure_link = { secret | secret_env, signature, expires }`; HMAC-SHA256 over path and expiry (not nginx's MD5 format), 403 or 410 |
| slice | todo | single ranges are answered from cached objects (written, unverified); fetching and caching fixed slices is not written |
| split_clients | done | `add_split` |
| ssi | skip | |
| ssl (certificates, SNI, session cache and tickets, client verify, OCSP staple files) | done | |
| ssl: OCSP responses fetched from the CA | skip | the large public CAs retired OCSP; stapling from files stays |
| stub_status, api, dashboard (Plus) | done | Prometheus metrics, `/state`, journeys |
| sub (`sub_filter`) | done | route `replace` |
| upstream: round robin, weights, `backup`, `down`, `max_fails`, `fail_timeout` | done | |
| upstream: `least_conn`, `ip_hash`, `hash`, `random` | done | |
| upstream: `least_time`, `sticky`, `slow_start`, `health_check` (Plus) | done | |
| upstream: `keepalive` | done | pooled connections |
| upstream: `resolve`, DNS re-resolution | done | |
| upstream: DNS SRV (`service=`) | wip | `add_upstream(name, { srv = "_http._tcp.name" })` over `hickory-resolver`; lowest priority group, refreshed with the names; record weights are not used |
| upstream: health check that matches the body | wip | `health = { path, status, body }`; plain text is contained, `~` starts a pattern |
| upstream: runtime changes through an API (Plus) | done | `/upstreams`; surviving a reload is todo |
| upstream changes that survive reloads | wip | API edits are an overlay re-applied on every reload; `POST /api/v1/upstreams/reset` forgets them; not kept across a restart |
| upstream: `queue` (Plus), `ntlm` (Plus) | gap | |
| v2 (HTTP/2) | done | |
| v3 (HTTP/3) | done | |
| WebSocket proxying | done | |
| WebSocket over HTTP/2 (RFC 8441) | wip | extended CONNECT is bridged to an HTTP/1 upgrade; HTTP/3 is not |

## nginx — stream and mail

| item | status | AegisX |
|---|---|---|
| stream core, proxy, upstream balancing | done | `add_stream` |
| stream: UDP | done | `udp = true` |
| stream: `ssl_preread` (SNI routing) | done | `sni = { name = pool }` |
| stream: PROXY protocol to the backend | done | |
| stream: TLS termination and TLS to backends | gap | streams are passed through |
| stream: `limit_conn`, access lists | wip | `add_stream { acl = { allow, deny }, max_connections }`; the limit is per listener, not per address |
| stream: map, geo, split_clients, js, return, set | gap | |
| mail proxy (IMAP, POP3, SMTP) | skip | not a web proxy concern |

## Caddy — directives

| item | status | AegisX |
|---|---|---|
| abort | wip | route `abort = true` closes the connection without an answer on HTTP/1 and resets the stream on HTTP/2; HTTP/3 answers empty |
| acme_server | skip | AegisX is an ACME client |
| basic_auth | done | |
| bind | done | |
| encode (gzip, zstd, br), precompressed | done | |
| error | done | `respond { status }` |
| file_server, browse | done | |
| forward_auth | done | |
| fs (virtual file systems) | skip | |
| handle, handle_path, route | done | routes, `strip_prefix` |
| handle_errors | done | `error_pages` |
| header, request_header | done | |
| import | done | `include` |
| intercept, `handle_response` | done | `error_pages` with interception, `X-Accel-Redirect` |
| invoke, named routes | skip | Lua functions give reuse |
| log, log formats and sinks | done | |
| log_append, log_skip, log_name | wip | route `log = false`; no appended fields or named logs |
| map | done | |
| method | wip | route `method = "GET"` |
| metrics | done | |
| php_fastcgi | wip | `php_app { host, root, fastcgi }` |
| push | skip | removed from browsers |
| redir | done | |
| request_body (`max_size`) | done | |
| respond | done | |
| reverse_proxy: policies, health, retries, headers, streaming | done | |
| reverse_proxy: dynamic upstreams (A, SRV) | wip | A done, SRV written |
| reverse_proxy: circuit breaking by latency | wip | `set_balancer(name, { slow_ms })`: an answer slower than that counts toward `max_fails` |
| rewrite, uri (strip, replace, regex) | done | |
| root | done | |
| templates | skip | |
| tls: manual certificates, client auth, ALPN | done | |
| tls: automatic HTTPS (HTTP-01, TLS-ALPN-01, DNS-01) | done | |
| tls: `internal` (local CA) | wip | `set_tls { internal = true, ca_dir, leaf_days }` over `rcgen`; root kept on disk |
| tls: on-demand | wip | `on_demand = { names, ask, capacity, per_hour }`; local authority mints in the handshake, ACME issues before it (TCP only) |
| tls: protocols and cipher suites selection | wip | `min_version = "1.2" | "1.3"`; suites stay the rustls set on purpose |
| tls: ECH | skip | not in rustls server yet |
| tracing | done | `traceparent`, OTLP export |
| try_files | done | |
| vars | done | variables from map, geo, split |

## Caddy — matchers and global options

| item | status | AegisX |
|---|---|---|
| matcher: path, path_regexp | done | |
| matcher: host | done | |
| matcher: method | done | |
| matcher: header, header_regexp | done | `match_headers` with `~regex` |
| matcher: query | done | `match_query` |
| matcher: remote_ip, client_ip | done | `acl` |
| matcher: protocol | wip | route `scheme = "http" | "https"`; no grpc or version matcher |
| matcher: file | done | `try_files` |
| matcher: vars, vars_regexp | done | `match_vars` |
| matcher: not | wip | a leading `!` negates any match value |
| matcher: expression (CEL) | wip | Lua `on_request` decides |
| option: admin API | done | control API with tokens |
| option: persist_config (API changes survive) | wip | reloads yes, restarts no |
| option: auto_https, email, acme_ca, storage | done | |
| option: on_demand_tls (ask endpoint, limits) | wip |  |
| option: pki (internal CA) | wip |  |
| option: servers timeouts, protocols, trusted_proxies | done | |
| option: grace_period, shutdown_delay | done | `drain_ms` |
| option: log | done | |
| option: default_sni, strict_sni_host | gap | |
| option: order | skip | route order is explicit |

## Beyond both

| item | status | AegisX |
|---|---|---|
| per-request journeys through a control API | done | |
| per-backend counters, latency and health in metrics and `/state` | done | |
| GeoIP `.mmdb` | wip | counted under nginx geoip |
| Lua presets `php_app`, `spa`, `static_site` | wip |  |
| English-like Lua DSL | wip | `listen`, `upstream "name" { … }`, `site "host" { route "/path" { … }, … }` with `30s` / `10MB` / `10/s` units; every `add_route` field still works inside `route`; the `set_*` / `add_*` functions stay |

## Coverage

Refreshed by `python3 ../bench/coverage.py FEATURES.md`.

<!-- coverage:begin -->
| against | done | wip | todo | gap | skip | coverage |
|---|--:|--:|--:|--:|--:|--:|
| nginx | 78 | 18 | 1 | 5 | 16 | 76.5 % |
| Caddy | 38 | 15 | 0 | 1 | 7 | 70.4 % |
| both | 116 | 33 | 1 | 6 | 23 | 74.4 % |
<!-- coverage:end -->
