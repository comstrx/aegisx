use std::sync::atomic::Ordering;

use serde_json::{Value, json};

use crate::app::Model;
use crate::config::AnalysisMode;
use crate::config::base::consts::VERSION;
use crate::core::sys::Sys;
use crate::core::time::Clock;
use super::arch::Admin;

impl Admin {

    pub(super) fn config_version ( &self, snapshot: &crate::app::Snapshot ) -> String {

        format!("{:016x}", self.boot_ms ^ snapshot.version.wrapping_mul(0x9E37_79B9_7F4A_7C15))

    }

    pub(super) fn state ( &self ) -> Value {

        let runtime = self.state.load();
        let snapshot = &runtime.snapshot;
        let config = &snapshot.config;
        let summary = self.telemetry.summary();
        let now = runtime.pools.now_ms();
        let resources = Sys::resources();
        let previous = self.resources.lock().map(|mut slot| slot.replace(resources)).unwrap_or(None);
        let cpu = previous.and_then(|previous| Sys::cpu_percent(previous, resources));

        let routes: Vec<Value> = snapshot.routes.iter().map(|route| json!({
            "name"          : route.spec.name,
            "path"          : route.spec.path,
            "host"          : route.spec.host,
            "methods"       : route.spec.methods,
            "upstream"      : route.spec.upstream,
            "capture"       : route.policy.capture,
            "model"         : if route.policy.capture && config.analysis.mode != AnalysisMode::Off { "observe" } else { "off" },
            "rate_limit_10s": route.policy.rate_limit,
            "concurrency_limit": route.policy.concurrency,
            "deny"          : route.spec.deny,
            "exact"         : route.spec.exact,
        })).collect();

        let upstreams: Vec<Value> = runtime.pools.list.iter().map(|pool| json!({
            "name"    : pool.name,
            "policy"  : format!("{:?}", pool.policy),
            "backends": pool.backends.iter().map(|backend| json!({
                "address"   : backend.addr.to_string(),
                "origin"    : backend.origin.as_deref(),
                "healthy"   : backend.available(now),
                "active"    : backend.active.load(Ordering::Relaxed),
                "latency_us": backend.latency_us.load(Ordering::Relaxed),
                "responses" : backend.totals()[0],
                "failures"  : backend.totals()[1],
                "retries"   : backend.totals()[2],
                "weight"    : backend.weight,
                "backup"    : backend.backup,
                "down"      : backend.down,
            })).collect::<Vec<_>>(),
        })).collect();

        let ( recent, journeys, active, dropped ) = self.telemetry.observe();
        let mode = match config.analysis.mode { AnalysisMode::Off => "off", AnalysisMode::Observe => "observe" };
        let model = self.analyser.as_ref().and_then(|analyser| Model::named(analyser.model()).ok());
        let ( feature_count, feature_version ) = model.as_ref().map_or(( 0, 0 ), |model| ( model.schema().count(), model.meta().feature_version ));

        let model = model.map(|model| json!({
            "model_version"     : model.meta().model_version,
            "input_schema"      : model.meta().input_schema,
            "precision"         : model.meta().precision,
            "parameter_count"   : model.meta().parameter_count,
            "source"            : model.meta().source,
            "deployment_ready"  : model.meta().deployment_ready,
            "evaluation_notice" : model.meta().evaluation_notice,
            "artifact_sha256"   : model.sha256(),
        }));

        json!({
            "version"        : VERSION,
            "config_version" : self.config_version(snapshot),
            "uptime_ms"      : Clock::elapsed_ms(self.started),
            "dropped_events" : dropped,
            "queue"          : { "capacity": 0, "waiting": 0, "timeout_ms": 0, "entered": 0, "completed": 0, "resumed": 0, "full": 0, "timed_out": 0, "unavailable": 0, "cancelled": 0, "total_us": 0 },
            "configuration"  : {
                "queue_capacity"              : 0,
                "queue_timeout_ms"            : 0,
                "threads"                     : config.worker_count(),
                "work_stealing"               : false,
                "accept_tasks"                : 1,
                "upstream_keepalive_capacity" : config.client.pool_capacity,
                "write_buffer_bytes"          : config.server.buffer,
                "keepalive_seconds"           : config.server.header_timeout_ms / 1_000,
                "scan_bytes"                  : config.analysis.scan_bytes,
                "response_scan_bytes"         : config.analysis.response_scan_bytes,
                "journey"                     : config.telemetry.enabled,
                "on_overload"                 : "skip",
                "feature_count"               : feature_count,
                "feature_version"             : feature_version,
                "route_count"                 : snapshot.routes.len(),
                "routes"                      : routes,
            },
            "resources"      : {
                "rss_bytes"           : resources.rss_bytes,
                "threads"             : resources.threads,
                "process_cpu_percent" : cpu,
                "logical_cpus"        : resources.cpus,
                "open_fds"            : resources.open_fds,
            },
            "storage"        : { "enabled": false, "healthy": true, "committed_events": 0, "committed_batches": 0, "pressure_rejections": 0, "dropped_batches": 0, "write_retries": 0, "used_slots": 0, "capacity": 0 },
            "telemetry"      : {
                "total"           : summary.total,
                "active"          : summary.active,
                "completed"       : summary.completed,
                "blocked"         : summary.blocked,
                "failed"          : summary.failed,
                "recent"          : recent,
                "latency_buckets" : summary.latency,
                "request_bytes"   : summary.request_bytes,
                "response_bytes"  : summary.response_bytes,
            },
            "cache"          : { "score_hits": 0, "response_hits": 0, "response_bytes": 0 },
            "policies"       : {
                "persistence_admission"   : "disabled",
                "persistence_synchronous" : "off",
                "model"                   : mode,
                "max_in_flight"           : config.limits.max_in_flight,
                "rate_limit_10s"          : config.limits.rate_limit_10s,
                "decision_cache"          : config.decisions.enabled,
                "response_cache"          : false,
                "deny_ttl_ms"             : config.decisions.deny_ttl_ms,
                "cancellable_routes"      : [],
            },
            "upstreams"      : upstreams,
            "webhooks"       : { "pending": 0, "delivered": 0, "failed": 0, "overflow": 0 },
            "analysis"       : self.analyser.as_ref().map_or(Value::Null, |analyser| analyser.state()),
            "decisions"      : self.state.decisions().map_or(Value::Null, |decisions| decisions.state()),
            "journeys"       : { "total": active, "items": journeys },
            "model"          : model.unwrap_or(Value::Null),
        })

    }

}
