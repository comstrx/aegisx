# Local Nginx comparison · 2026-10-02

AegisX's previous request-rate numbers did not establish its position against another reverse proxy. This run compares **the same native backend, host, worker count, body, downstream concurrency and test duration**, with the complete raw settings retained.

| Concurrent connections | Direct backend median | Nginx median | AegisX median | AegisX / Nginx |
| --- | ---: | ---: | ---: | ---: |
| 128 | 191,230/s | **76,720/s** | **32,090/s** | 41.8% |
| 256 | 144,759/s | **71,007/s** | **28,065/s** | 39.5% |

There is a substantial forwarding-performance gap in this local configuration. Reaching 27–32k/s here does not establish parity. These observations do not identify which AegisX or Pingora function causes the gap.

## Method and limits

- Existing Ubuntu **Nginx 1.24.0** package; this is a measurement of the installed build, not a claim about the newest Nginx release. Full build flags are recorded.
- AegisX v0.9.0, SHA-256 `bcc62dc6e43e7560d0d305f3690e8ba2b8e4140596d9195c13e741d5bbbb6c94`.
- Eight proxy workers each, two native backend workers and two wrk client threads on the same WSL host. No dedicated-host isolation or CPU affinity.
- HTTP/1.1 keepalive, fixed 128-byte body, 128 and 256 downstream connections; two-second warmup plus 15-second measurement, three trials per target/concurrency.
- Target order rotates between direct backend, Nginx and AegisX. Every result is retained, rather than choosing the highest run.
- No TLS, model, capture, decision/response cache or rate policy. Nginx request/response buffering is explicitly off; upstream keepalive capacity is 256 per worker, matching the configured AegisX limit. AegisX still propagates correlation/forwarding headers and retains its admission/protocol checks; this is not identical per-request work.
- No HTTP/socket errors were reported. Listener readiness verifies the expected 200 response and exact 128-byte body.
- Model training, model preflight, builds and test suites were not running during the comparison. Source editing continued; unrelated host workloads remained uncontrolled.
- The system Nginx service and its configuration were not modified. The script starts temporary foreground processes on free loopback ports and terminates only its own process groups.

Nginx documents [proxy buffering behavior](https://nginx.org/en/docs/http/ngx_http_proxy_module.html#proxy_buffering) and [upstream keepalive](https://nginx.org/en/docs/http/ngx_http_upstream_module.html#keepalive). Disabling buffering here preserves the intended streaming comparison; throughput gains from delaying streaming would be a different tradeoff.

## Reproduction and evidence

[Raw trials, latency output, configs, build flags and hashes](2026-10-02-nginx-comparison.json).

```sh
python3 server/tests/compare_nginx.py \
  --workers 8 --connections 128 256 --seconds 15 --trials 3 \
  --output server/benchmarks/new-nginx-comparison.json
```

The current production binary is unchanged by this research round. A future optimization must be profiled, tested for transport correctness and measured against this baseline. The comparison itself is not evidence of a speed improvement.
