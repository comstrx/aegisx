# AegisX panel · 0.8

Next.js 16.3.8 / React 19.3.0 / TypeScript 7.0.2 static export, embedded in the Rust executable. No Node process in production.

Enable `set_control {enabled=true,listen="127.0.0.1:9090",token_env="AEGISX_ADMIN_TOKEN"}` and supply a random 32–4096-character printable ASCII token in the service environment. The token stays only in tab memory; refresh/disconnect discards it.

Views: Overview, Requests, Decisions, Upstreams Analysis and Configuration. Actual one-second polling drives request-rate/blocked charts, lifetime latency histogram, request search/filter, active and retained lifecycle timelines, backend operation reports, durable decisions/revocation, cooperative cancellation states, upstream readiness/load/EWMA latency, queue pressure, inference latency, cache effectiveness, webhook counters and process RSS/CPU/threads. CPU 100% means one logical core; these are Linux process metrics, not all-host or container-quota accounting.

Request actions obey Lua capabilities and duration limits. Manual bans apply to future admissions. Safe cancellation is available only for explicitly enabled routes, and the panel distinguishes requested/accepted from cancelled/too_late/unsupported. No AI result aborts an already-forwarded request.

Charts use cumulative server counters, with no fabricated samples. Poll failures expose stale data; bounded history, sampling, retention and queue drops are visible limitations. Backend internal execution appears only when instrumented reports arrive. This dashboard is not Grafana feature parity, a lossless tracing system, or a general metrics database.

```sh
# Repository root; pinned Node available in PATH or panel/.runtime:
python3 scripts/build.py
```

`server/build.rs` embeds the export and rejects missing assets. `app/` owns layout/style; `components/` owns presentation; `lib/client.ts` owns transport/types; `lib/runtime.ts` owns polling/rate calculation. Use npm ci/check/build for development. The real API is served by Rust at the bootstrap-provided prefix; next dev alone does not provide it.

Browser QA: Python 3.14 + [tests/requirements.txt](tests/requirements.txt), compatible Chromium, then `python panel/tests/browser.py` after building. `AEGISX_BIN` and `CHROMIUM_PATH` can select test executables. The test verifies login, actual runtime data, controls, timeline, mobile layout and token isolation, and saves screenshots/QA JSON to tmp/. Python and Playwright are test-only dependencies.

The capture/durability card separates admitted reservations, committed events/journeys, pre-forwarding pressure rejections, best-effort omissions and write retries. Analysis shows whether the numeric journal is enabled and how many interrupted jobs were reconciled without replaying enforcement (text jobs are unavailable because raw inputs are volatile). Storage failures are explicit. Disk durability starts at commit; active/queued volatile work is not presented as safely persisted.

The panel supplies a restrained teal identity, a fixed desktop shell and scrollable mobile navigation, light/dark themes, actual counter sparklines, accessible SVG charts, searchable/paged request tables, native modal forms and a journey drawer. Explicit loading, empty, stale/error, success-toast and persistent failure states are present. Reduced-motion preferences disable decorative animation; keyboard / focuses request search and Escape closes dialogs.

Block forms require a reason and bounded duration; revoke/cancellation forms explain their actual scope. Failed actions remain visible inside the modal. Pause freezes automatic polling; explicit successful actions can refresh once while paused. Disconnect aborts in-flight work and clears the token and views. The Configuration view shows safe effective values and a read-only Lua preview. No online editor, fake apply control or synthetic data is presented.

Chromium QA verifies real block/revoke backend statuses, dark/light views, pause/resume, search and empty results, keyboard/dialog behavior, effective configuration, embedded model/schema, capture/journal counters, mobile overflow, invalid-token recovery and token isolation. Evidence is written to tmp/panel-v08-*.png and tmp/panel-qa.json.

Version 0.7 adds a white light-mode sidebar, rounded cards/fields/buttons, layered surface shadows, dimensional chart/ring rendering and entrance/exit motion with reduced-motion overrides. The traffic waiting room uses real admission-queue counters; it distinguishes waiting capacity from analysis-job capacity and shows full/timeout outcomes. Chart values and axes are unchanged by decorative depth. Browser coverage includes 320, 390, 768, 1024 and 1440 pixel widths.


Version 0.8 displays the text/journey schema, separate content/journey signals and the actual input byte/event coverage in request details. Missing response/events and truncated samples are explicit. The diagnostics view explains why interrupted text jobs cannot be replayed from the numeric journal. Research scores are not presented as calibrated compromise probabilities.
