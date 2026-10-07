use std::net::SocketAddr;
use ipnet::IpNet;
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct CacheConfig {
    pub decisions: bool,
    pub lookup_timeout_ms: u64,
    pub write_timeout_ms: u64,
    pub decision_ttl_ms: u64,
    pub on_lookup_failure: String,
    pub scores: bool,
    pub responses: bool,
    pub max_entries: u64,
    pub max_bytes: u64,
    pub max_object_bytes: usize,
    pub deny_ttl_ms: u64,
    pub score_ttl_ms: u64,
    pub response_ttl_ms: u64,
    pub background_denials: bool,
}
impl Default for CacheConfig {
    fn default () -> Self {
        Self { lookup_timeout_ms: 25, write_timeout_ms: 1000, decision_ttl_ms: 1000, on_lookup_failure: "deny".into(), decisions: false, scores: false, responses: false, max_entries: 10000,
            max_bytes: 16777216, max_object_bytes: 262144, deny_ttl_ms: 5000,
            score_ttl_ms: 1000, response_ttl_ms: 10000, background_denials: false }
    }
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct IdentityConfig {
    pub request_id_header: String,
    pub propagate: bool,
    pub preserve_trusted_id: bool,
    pub actor_header: Option<String>,
    pub trusted_peers: Vec<IpNet>,
    pub forwarding: bool,
    pub forwarded_for_header: String,
    pub forwarded_proto_header: String,
    pub backend_block_header: Option<String>,
}
impl Default for IdentityConfig {
    fn default () -> Self {
        Self { request_id_header: "x-request-id".into(), propagate: true, preserve_trusted_id: false,
            actor_header: None, trusted_peers: Vec::new(), forwarding: true,
            forwarded_for_header: "x-forwarded-for".into(), forwarded_proto_header: "x-forwarded-proto".into(),
            backend_block_header: None }
    }
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct TelemetryConfig {
    pub enabled: bool,
    pub recent_capacity: usize,
    pub capture: String,
    pub sample_every: u64,
}
impl Default for TelemetryConfig {
    fn default () -> Self { Self { enabled: true, recent_capacity: 256, capture: "full".into(), sample_every: 1 } }
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct ControlConfig {
    pub enabled: bool,
    pub panel: bool,
    pub listen: SocketAddr,
    pub api_prefix: String,
    pub token_env: String,
    pub backend_token_env: Option<String>,
}
impl Default for ControlConfig {
    fn default () -> Self {
        Self { enabled: false, panel: true, listen: ([127, 0, 0, 1], 9090).into(),
            api_prefix: "/api/v1".into(), backend_token_env: None, token_env: "AEGISX_ADMIN_TOKEN".into() }
    }
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct WebhookConfig {
    pub enabled: bool,
    pub queue_capacity: usize,
    pub timeout_ms: u64,
    pub attempts: u32,
    pub endpoints: Vec<WebhookEndpoint>,
}
impl Default for WebhookConfig {
    fn default () -> Self { Self { enabled: false, queue_capacity: 128, timeout_ms: 1000, attempts: 3, endpoints: Vec::new() } }
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct WebhookEndpoint {
    pub name: String,
    pub url: String,
    pub secret_env: String,
    #[serde(default)]
    pub events: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct ContextConfig { pub shards: usize, pub idle_ttl_ms: u64 }

impl Default for ContextConfig {
    fn default () -> Self { Self { shards: 16, idle_ttl_ms: 120000 } }
}

impl CacheConfig {
    pub fn resources ( &self ) -> (u64, u64, usize, u64, u64, u64) {
        (self.max_entries, self.max_bytes, self.max_object_bytes, self.deny_ttl_ms, self.score_ttl_ms, self.response_ttl_ms)
    }
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct PersistenceConfig { pub admission: String, pub synchronous: String }
impl Default for PersistenceConfig {
    fn default () -> Self { Self { admission: "required".into(), synchronous: "full".into() } }
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct QueueConfig { pub capacity: usize, pub timeout_ms: u64 }
impl Default for QueueConfig {
    fn default () -> Self { Self { capacity: 0, timeout_ms: 1000 } }
}
