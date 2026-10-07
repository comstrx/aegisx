use std::collections::BTreeMap;
use std::net::SocketAddr;
use std::path::PathBuf;

use serde::Deserialize;

#[derive(Debug, Clone)]
pub struct Config {
    pub queue: super::QueueConfig,
    pub persistence: super::options::PersistenceConfig,
    pub context: super::options::ContextConfig,
    pub cache: super::options::CacheConfig,
    pub identity: super::options::IdentityConfig,
    pub telemetry: super::options::TelemetryConfig,
    pub control: super::options::ControlConfig,
    pub webhooks: super::options::WebhookConfig,
    pub listen: SocketAddr,
    pub upstream: SocketAddr,
    pub default_pool: Option<String>,
    pub pools: BTreeMap<String, PoolConfig>,
    pub routes: Vec<Route>,
    pub model: ModelConfig,
    pub store: Option<PathBuf>,
    pub limits: Limits,
    pub runtime: RuntimeConfig,
    pub tls: Option<TlsConfig>,
    pub preserve_host: bool,
    pub request_headers: BTreeMap<String, String>,
    pub response_headers: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct Limits {
    pub timeout_ms: u64,
    pub max_body_bytes: usize,
    pub max_sources: usize,
    pub queue_capacity: usize,
    pub retention_events: usize,
    pub rate_limit_10s: u64,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct RuntimeConfig {
    pub threads: usize,
    pub work_stealing: bool,
    pub accept_tasks: usize,
    pub upstream_keepalive_capacity: usize,
    pub max_in_flight: usize,
    pub write_buffer_bytes: usize,
    pub keepalive_seconds: u64,
    pub pool_idle_seconds: u64,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct TlsConfig {
    pub cert: PathBuf,
    pub key: PathBuf,
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum Mode { Off, Observe, Enforce, Background }

#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct ModelConfig {
    pub mode: Mode,
    pub scan_bytes: usize,
    pub response_scan_bytes: usize,
    pub journey: bool,
    pub on_overload: String,
    pub max_queue_age_ms: u64,
    pub directory: Option<PathBuf>,
    pub threshold: f32,
    pub content_threshold: Option<f32>,
    pub journey_threshold: Option<f32>,
    pub queue_capacity: usize,
    pub allow_unvalidated: bool,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct BackendConfig {
    pub address: SocketAddr,
    pub weight: u32,
    pub max_in_flight: u64,
    pub tls: bool,
    pub server_name: String,
    pub ca_file: Option<PathBuf>,
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum Balance { RoundRobin, LeastConn, Adaptive, First }

#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct PoolOptions {
    pub policy: Balance,
    pub max_fails: u32,
    pub cooldown_ms: u64,
    pub health_interval_ms: u64,
    pub health_timeout_ms: u64,
    pub health_path: Option<String>,
    pub health_status: u16,
    pub connect_attempts: usize,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct PoolConfig {
    pub backends: Vec<BackendConfig>,
    pub options: PoolOptions,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Route {
    pub name: String,
    pub host: Option<String>,
    pub path: String,
    pub exact: bool,
    pub methods: Vec<String>,
    pub match_headers: BTreeMap<String, String>,
    pub upstream: String,
    pub response_cache: Option<bool>,
    pub decision_cache: Option<bool>,
    pub strip_prefix: bool,
    pub cancellation: bool,
    pub cancellation_ttl_ms: u64,
    pub deny: bool,
    pub timeout_ms: Option<u64>,
    pub max_body_bytes: Option<usize>,
    pub rate_limit_10s: Option<u64>,
    pub model: Option<Mode>,
    pub threshold: Option<f32>,
    pub capture: Option<bool>,
    pub preserve_host: Option<bool>,
    pub request_headers: BTreeMap<String, String>,
    pub response_headers: BTreeMap<String, String>,
}
