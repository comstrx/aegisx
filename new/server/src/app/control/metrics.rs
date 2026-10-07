use std::fmt::Write;
use std::sync::atomic::Ordering;

use crate::config::base::consts::VERSION;
use crate::http::body::Body;
use crate::http::response::{Res, Response};
use crate::app::telemetry::BUCKETS;
use super::arch::Admin;

impl Admin {

    pub(super) fn metrics ( &self ) -> Res<Body> {

        let runtime = self.state.load();
        let summary = self.telemetry.summary();
        let now = runtime.pools.now_ms();
        let files = self.state.files();
        let mut out = String::with_capacity(4_096);

        let _ = writeln!(out, "# TYPE aegisx_build_info gauge\naegisx_build_info{{version=\"{VERSION}\"}} 1");
        let _ = writeln!(out, "# TYPE aegisx_uptime_seconds gauge\naegisx_uptime_seconds {}", self.started.elapsed().as_secs());
        let _ = writeln!(out, "# TYPE aegisx_config_version gauge\naegisx_config_version {}", runtime.snapshot.version);

        for ( name, value ) in [( "total", summary.total ), ( "completed", summary.completed ), ( "blocked", summary.blocked ), ( "failed", summary.failed )] {

            let _ = writeln!(out, "# TYPE aegisx_requests_{name} counter\naegisx_requests_{name} {value}");

        }

        let _ = writeln!(out, "# TYPE aegisx_requests_active gauge\naegisx_requests_active {}", summary.active);
        let _ = writeln!(out, "# TYPE aegisx_request_bytes_total counter\naegisx_request_bytes_total {}", summary.request_bytes);
        let _ = writeln!(out, "# TYPE aegisx_response_bytes_total counter\naegisx_response_bytes_total {}", summary.response_bytes);
        let _ = writeln!(out, "# TYPE aegisx_request_duration_milliseconds histogram");

        let mut cumulative = 0;

        for ( index, limit ) in BUCKETS.iter().enumerate() {

            cumulative += summary.latency[index];

            let _ = writeln!(out, "aegisx_request_duration_milliseconds_bucket{{le=\"{limit}\"}} {cumulative}");

        }

        cumulative += summary.latency[BUCKETS.len()];

        let _ = writeln!(out, "aegisx_request_duration_milliseconds_bucket{{le=\"+Inf\"}} {cumulative}\naegisx_request_duration_milliseconds_count {cumulative}");
        let _ = writeln!(out, "# TYPE aegisx_backend_up gauge\n# TYPE aegisx_backend_active_requests gauge\n# TYPE aegisx_backend_latency_microseconds gauge\n# TYPE aegisx_backend_responses_total counter\n# TYPE aegisx_backend_failures_total counter\n# TYPE aegisx_backend_retries_total counter");

        for pool in &runtime.pools.list {

            for backend in &pool.backends {

                let labels = format!("pool=\"{}\",backend=\"{}\"", pool.name, backend.addr);

                let _ = writeln!(out, "aegisx_backend_up{{{labels}}} {}", u8::from(backend.available(now)));
                let _ = writeln!(out, "aegisx_backend_active_requests{{{labels}}} {}", backend.active.load(Ordering::Relaxed));
                let _ = writeln!(out, "aegisx_backend_latency_microseconds{{{labels}}} {}", backend.latency_us.load(Ordering::Relaxed));

                for ( name, value ) in ["responses", "failures", "retries"].into_iter().zip(backend.totals()) {

                    let _ = writeln!(out, "aegisx_backend_{name}_total{{{labels}}} {value}");

                }

            }

        }

        let _ = writeln!(out, "# TYPE aegisx_file_cache_entries gauge\naegisx_file_cache_entries {}\n# TYPE aegisx_file_cache_bytes gauge\naegisx_file_cache_bytes {}", files.entries(), files.bytes());

        if let Some(store) = self.state.cache() {

            let described = store.describe();

            for name in ["entries", "bytes", "hits", "misses", "stale", "filled"] {

                let kind = if matches!(name, "entries" | "bytes") { "gauge" } else { "counter" };

                let _ = writeln!(out, "# TYPE aegisx_cache_{name} {kind}\naegisx_cache_{name} {}", described.get(name).and_then(|value| value.as_u64()).unwrap_or(0));

            }

        }

        let mut response = Response::bytes(200, "text/plain; version=0.0.4; charset=utf-8", out);

        Self::harden(response.headers_mut());

        response

    }

}
