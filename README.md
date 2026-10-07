# ✨ AegisX

A local reverse proxy that connects request lifecycles, behavioral context and explicit security policies to actual traffic decisions.

**One Linux executable. One Lua configuration. Optional analysis.** Rust forwards traffic and serves an embedded Next.js control panel; an embedded ONNX model runs locally. No Redis, database server, Python service, Node service or cloud account is required in production.

## Implemented · 0.10

- HTTP/1.1 streaming, TLS termination and verified upstream TLS.
- Host/wildcard/path/method/header routing, rewrites, headers and route-specific policy.
- Weighted, least-active, adaptive-latency and ordered-failover balancing; capacity limits; passive and TCP/HTTP/HTTPS active health checks.
- Post-completion background inference, bounded sharded numeric context and global/route rate limits; no inline ML wait.
- SQLite-backed expiring decisions with coherent positive/negative caching; independent exact-score and public-response caches.
- Configurable correlation headers, trusted actor integration and backend signals affecting future requests.
- Signed, queued webhooks with bounded retries and visible delivery/drop counters.
- SQLite lifecycle capture, bounded backend operation reports, safe cancellation intents/acknowledgements and an embedded operational dashboard.
- Python training with group-separated evaluation, mini-batches, class balancing, gradient clipping, resumable checkpoints, early stopping and ONNX parity checks.

Version 0.7 adds a **bounded traffic waiting room** for global/upstream capacity and required storage/analysis reservations. Lua controls its capacity and shared per-request deadline. Wakeups are driven by resource release, health recovery and passive cooldown expiry. Waiting requests recheck committed bans before forwarding; an already-forwarded request is never retroactively cut off by a model verdict.

The hot path reuses compiled headers, request-ID values and model input buffers, with fewer unnecessary allocations and clock reads. The panel gains a white light-mode sidebar, rounded layered surfaces, real queue metrics and visible model evaluation limitations. Configuration, transport correctness and measured outcomes take priority over speculative optimization.

Version 0.9 selects a **895,178-parameter compact student** after three real training candidates, including a **125.64M-parameter transfer teacher** (124.65M frozen pretrained parameters plus a trained classification head). The selected model remains byte/text + ordered journey + numeric context; its UINT8 artifact is **1.22 MB**. Three thousand additional executed SQLite transactions teach distinctions between authorization, unsafe commits and safe rollbacks.

**Research/observe only.** At a 0.1% validation FPR budget, internal content recall improves from 81.22% to 98.39%, with zero observed false positives among 975 benign test rows. External CRS recall remains only 9.25% and external benign FPR is 0.093%; broader production detection is unproven. See [model evidence](model/README.md) and [0.9 verification](server/benchmarks/v09.md).

Rust now coalesces concurrent durable-decision lookups, validates stored decision integrity, avoids copying complete cached verdicts, maintains rolling totals without per-request bucket scans, and preserves normalization parity with faster extraction. Lua supports separate content/journey thresholds. All inference stays after request completion.

## Version 0.10

Admission uses exact atomic capacity accounting with cancellation-safe wakeups; unused upstream counters are skipped when the control plane and adaptive balancing do not need them. Small downstream write batches can be coalesced without waiting for later streaming chunks. A narrow timer patch removes a redundant allocation. Transport patches are pinned under server/vendor, licensed and tested.

The new [395M-parameter experiment](model/research/unified-v2.md) includes actual full-backbone fine-tuning, expanded execution oracles and resumable training. It fails its quality prerequisite and remains a research artifact; the embedded compact model is unchanged. Server changes and performance evidence are documented separately from model parameter count.

Measured forwarding medians improve 2.7–5.7% in this local run, while the policy-profile median regresses 2.5%. Memory does not consistently fall, and throughput remains below half of the installed Nginx build. See [0.10 verification and all trials](server/benchmarks/v010.md).

## Current research direction

A subsequent [unified-model research pilot](model/research/unified-v1.md) downloaded ModernBERT, jointly trained text/numeric/journey inputs through one backbone and reached full-network fine-tuning. Its short training run did not pass the validation prerequisite, so it does not replace the embedded v0.9 model. The new executable SQLite fixtures, split/calibration controls and checkpoints are reproducible.

An [actual local Nginx comparison](server/benchmarks/nginx-comparison.md) puts AegisX at 39.5–41.8% of the installed Nginx build's forwarding throughput in the measured eight-worker profiles. This establishes a gap to investigate, not competitive parity.

## Build and run

Use Ubuntu/Linux, the repository's Rust/Python pins and Node from `panel/.nvmrc`.

```sh
python3 scripts/build.py
./dist/aegisx --config server/Aegisx.lua --check
./dist/aegisx --config server/Aegisx.lua
```

The supplied configuration forwards port 8080 to your application on 3000. Enable the local panel using `set_control` and an admin token supplied through the service environment. The binary embeds panel assets, model, ONNX CPU runtime, Lua, SQLite and OpenSSL. Standard Linux C/C++ runtime libraries are still required; this is not a fully static executable.

## Workspace cleanup · 2026-10-02

The workspace was reduced from 36.75 GiB to about 1.91 GiB. See [cleanup-report.json](cleanup-report.json) for the exact removal inventory. Source, lockfiles, licenses, the current dist executable, embedded ONNX, final Word/PDF presentation, measured reports and recorded SQLite oracle examples remain.

Only the latest unified-v2 selected research checkpoint and its prepared data are retained from the large research binaries/tensors. Older checkpoints, duplicate exports, downloaded pretrained weight files, bulk WCP downloads, profiler recordings, temporary binaries, Node runtime, node_modules, panel build output, Rust target and Python virtual environment/caches were removed. Historical reports describe the artifacts measured at that time; removed checkpoints and old benchmark executables are no longer available locally.

The current dist/aegisx runs independently; relocation/model/panel/API checks passed after cleanup. To develop again, provide the Node version in panel/.nvmrc and run python3 scripts/build.py to regenerate panel dependencies/export and Rust output. Recreate the Python environment with uv sync --project model --locked. Research training may also need the pinned downloads restored with the existing fetch scripts. The retained 395M research checkpoint still has not passed deployment quality gates.

The legacy numeric compatibility fixture now lives under server/tests/fixtures/numeric-v4, so its test no longer depends on temporary files.

## Architecture

```text
model/   Python: validated data → training → evaluation → ONNX
panel/   Next.js static export → embedded assets
server/  One Rust crate:
         core    → shared errors, numeric facts and integration types
         module  → config / runtime / identity / memory / decision / cache
                   inference / lifecycle / verdict / upstream / queue / storage / telemetry / webhook / control / proxy
         app     → startup, service lifecycle and CLI
```

`Services` owns shared dependencies once. HTTP adapters extract facts and apply decisions; the decision engine owns history and inference policy. Immutable configuration snapshots keep in-flight requests consistent across SIGHUP reload. Explicit Lua settings control automation.

## Documentation

- [Server, transport and Lua](server/README.md)
- [Caching, context and new configuration](server/configuration.md)
- [Backend, webhook and control contracts](server/contracts/README.md)
- [Model pipeline and data](model/README.md)
- [Embedded panel](panel/README.md)
- [Measured local performance and limits](server/benchmarks/README.md)

## Scope and direction

The goal is an independent proxy competitive for a defined deployment scope, then broader NGINX/Caddy capabilities and Kubernetes integration. It is currently local and single-process; there is no Kubernetes Ingress/Gateway controller.

Future work includes automatic certificate management, broader protocols, durable webhook delivery, deeper labeled behavioral learning and broader infrastructure integration. A reverse proxy cannot see arbitrary backend code execution or undo completed operations. Backend job cancellation requires explicit route opt-in and a cooperating receiver; acknowledgement and actual cancellation are separate states.

Capture and analysis queues are bounded. The supplied Lua enables up to 256 waiting episodes with a 1,000 ms deadline; the library default leaves waiting disabled. When waiting is disabled, full or expired, required reservations reject before forwarding. This is a live HTTP waiting queue, not durable body spooling or unlimited capacity. Explicit best-effort/skip policies are available. Webhooks remain best effort. Committed decisions, cancellation intents and numeric analysis jobs survive restart; active/uncommitted work is volatile. Interrupted text-model jobs are explicitly unavailable because the numeric journal does not persist their text inputs. Model predictions are not training labels. Training and central upload never happen automatically.

## Community

- [Issues](https://github.com/comstrx/aegisx/issues)
- [Discussions](https://github.com/comstrx/aegisx/discussions)
- [Contributing](https://github.com/comstrx/aegisx/blob/main/CONTRIBUTING.md)
- [Security](https://github.com/comstrx/aegisx/blob/main/SECURITY.md)
- [Support](https://github.com/comstrx/aegisx/blob/main/SUPPORT.md)

## License

Copyright © 2026 Abdulrahman Yasser (comstrx).

Licensed under the [Apache License, Version 2.0](./LICENSE).
