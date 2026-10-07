use super::arch::*;

impl Default for Limits {
    fn default () -> Self {
        Self { timeout_ms: 10000, max_body_bytes: 1048576, max_sources: 10000,
            queue_capacity: 1024, retention_events: 100000, rate_limit_10s: 0 }
    }
}

impl Default for RuntimeConfig {
    fn default () -> Self {
        Self { threads: 2, work_stealing: false, accept_tasks: 1, upstream_keepalive_capacity: 128, max_in_flight: 4096, write_buffer_bytes: 4096,
            keepalive_seconds: 30, pool_idle_seconds: 30 }
    }
}

impl Default for ModelConfig {
    fn default () -> Self {
        Self { on_overload: "reject".into(), scan_bytes: 16384, response_scan_bytes: 0, journey: true, max_queue_age_ms: 5000, mode: Mode::Off, directory: None, threshold: 0.95,
            content_threshold: None, journey_threshold: None, queue_capacity: 64, allow_unvalidated: false }
    }
}

impl Default for BackendConfig {
    fn default () -> Self {
        Self { address: ([127, 0, 0, 1], 3000).into(), weight: 1,
            max_in_flight: 0, tls: false, server_name: String::new(), ca_file: None }
    }
}

impl Default for PoolOptions {
    fn default () -> Self {
        Self { policy: Balance::RoundRobin, max_fails: 2, cooldown_ms: 5000,
            health_interval_ms: 0, health_timeout_ms: 300, health_path: None, health_status: 200, connect_attempts: 1 }
    }
}

impl Default for Route {
    fn default () -> Self {
        Self { name: String::new(), host: None, path: "/".into(), exact: false,
            response_cache: None, decision_cache: None, methods: Vec::new(), match_headers: Default::default(), upstream: String::new(), strip_prefix: false, cancellation: false, cancellation_ttl_ms: 60000, deny: false,
            timeout_ms: None, max_body_bytes: None, rate_limit_10s: None, model: None,
            threshold: None, capture: None, preserve_host: None,
            request_headers: Default::default(), response_headers: Default::default() }
    }
}

impl Default for Config {
    fn default () -> Self {
        Self { queue: Default::default(), persistence: Default::default(), context: Default::default(), cache: Default::default(), identity: Default::default(), telemetry: Default::default(), control: Default::default(), webhooks: Default::default(), listen: ([127, 0, 0, 1], 8080).into(), upstream: ([127, 0, 0, 1], 3000).into(),
            default_pool: Some("default".into()), pools: Default::default(), routes: Vec::new(),
            model: ModelConfig::default(), store: None, limits: Limits::default(),
            runtime: RuntimeConfig::default(), tls: None, preserve_host: false,
            request_headers: Default::default(), response_headers: Default::default() }
    }
}
