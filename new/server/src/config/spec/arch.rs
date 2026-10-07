use std::collections::BTreeMap;
use std::net::SocketAddr;
use std::path::PathBuf;

use ipnet::IpNet;
use serde::Deserialize;

use crate::core::net::{Address, Wire};
use crate::http::h3::Congestion;
use crate::http::server::Accept;
use crate::http::upstream::Protocol;
use crate::http::variable::Recipe;

#[derive(Debug, Clone, PartialEq)]
pub struct Config {
    pub listen           : SocketAddr,
    pub listen_unix      : Option<String>,
    pub tls              : Option<TlsConfig>,
    pub runtime          : RuntimeConfig,
    pub server           : ServerConfig,
    pub client           : ClientConfig,
    pub limits           : Limits,
    pub identity         : IdentityConfig,
    pub log              : LogConfig,
    pub control          : ControlConfig,
    pub telemetry        : TelemetryConfig,
    pub models           : BTreeMap<String, ModelSpec>,
    pub analysis         : AnalysisConfig,
    pub decisions        : DecisionConfig,
    pub access           : AccessConfig,
    pub compression      : CompressionConfig,
    pub files            : FilesConfig,
    pub cache            : CacheConfig,
    pub http3            : Http3Config,
    pub default_pool     : Option<String>,
    pub pools            : BTreeMap<String, PoolConfig>,
    pub routes           : Vec<Route>,
    pub request_headers  : BTreeMap<String, String>,
    pub response_headers : BTreeMap<String, String>,
    pub preserve_host    : bool,
    pub error_pages      : Vec<ErrorPage>,
    pub streams          : Vec<StreamConfig>,
    pub variables        : BTreeMap<String, Recipe>,
    pub jwts             : BTreeMap<String, JwtConfig>,
    pub listeners        : Vec<ListenConfig>,
    pub acl              : Acl,
    pub hooks            : HookSpec,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct HookSpec {
    pub request  : bool,
    pub response : bool,
    pub source   : String,
    pub name     : String,
    pub base     : PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ListenConfig {
    pub address        : SocketAddr,
    pub tls            : bool,
    pub redirect       : bool,
    pub proxy_protocol : bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Acl {
    #[serde(deserialize_with = "nets")]
    pub allow : Vec<IpNet>,
    #[serde(deserialize_with = "nets")]
    pub deny  : Vec<IpNet>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Respond {
    pub status       : u16,
    pub body         : String,
    pub content_type : String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct StreamConfig {
    pub name           : String,
    pub listen         : SocketAddr,
    pub upstream       : String,
    pub udp            : bool,
    pub idle_ms        : u64,
    pub proxy_protocol : Option<Wire>,
    pub sni            : BTreeMap<String, String>,
    pub acl            : Acl,
    pub max_connections: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct RuntimeConfig {
    pub workers        : usize,
    pub pin            : bool,
    pub backlog        : i32,
    pub accept         : Accept,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ServerConfig {
    pub keepalive         : bool,
    pub max_headers       : usize,
    pub max_connections   : usize,
    pub header_timeout_ms : u64,
    pub keepalive_timeout_ms : u64,
    pub keepalive_requests : u32,
    pub send_timeout_ms   : u64,
    pub keepalive_time_ms : u64,
    pub underscores_in_headers : bool,
    pub buffer            : usize,
    pub drain_ms          : u64,
    pub http2             : bool,
    pub h2c               : bool,
    pub proxy_protocol    : bool,
    pub max_streams       : u32,
    pub h2_adaptive_window: bool,
    pub h2_stream_window  : u32,
    pub h2_connection_window : u32,
    pub h2_max_frame      : u32,
    pub h2_max_header_bytes : u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ClientConfig {
    pub connect_timeout_ms : u64,
    pub pool_idle_ms       : u64,
    pub pool_capacity      : usize,
    pub buffer             : usize,
    pub attempts           : usize,
    pub resolve_ms         : u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct RateRule {
    pub key   : String,
    pub rate  : u32,
    pub burst : u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Limits {
    pub timeout_ms        : u64,
    pub client_timeout_ms : u64,
    pub max_body_bytes    : usize,
    pub max_in_flight     : usize,
    pub rate_limit_10s    : u32,
    pub rate_per_second   : u32,
    pub rate_burst        : u32,
    pub rate_key          : Option<String>,
    pub rate_rules        : Vec<RateRule>,
    pub concurrency_limit : u32,
    pub bandwidth         : u64,
    pub bandwidth_after   : u64,
    pub buffer_requests   : bool,
    pub spool_bytes       : usize,
    pub spool_dir         : PathBuf,
    pub buffer_responses  : bool,
    pub response_buffer_bytes : usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct IdentityConfig {
    pub request_id_header      : String,
    pub propagate              : bool,
    pub forwarding             : bool,
    pub forwarded_for_header   : String,
    pub forwarded_proto_header : String,
    #[serde(deserialize_with = "nets")]
    pub trusted_peers          : Vec<IpNet>,
    pub actor_header           : Option<String>,
    pub backend_block_header   : Option<String>,
    pub traceparent            : bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct LogConfig {
    pub level : String,
    pub json  : bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ControlConfig {
    pub enabled           : bool,
    pub listen            : SocketAddr,
    pub prefix            : String,
    pub token_env         : String,
    pub backend_token_env : Option<String>,
    pub panel             : bool,
    pub panel_dir         : Option<PathBuf>,
    pub body_bytes        : usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct TelemetryConfig {
    pub enabled  : bool,
    pub recent           : usize,
    pub journeys         : usize,
    pub otlp             : Option<String>,
    pub otlp_interval_ms : u64,
    pub otlp_headers     : BTreeMap<String, String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AnalysisMode {
    #[default]
    Off,
    Observe,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct AnalysisConfig {
    pub mode                : AnalysisMode,
    pub model               : String,
    pub scan_bytes          : usize,
    pub response_scan_bytes : usize,
    pub workers             : usize,
    pub capacity            : usize,
    pub deadline_ms         : u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AccessFormat {
    #[default]
    Combined,
    Json,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Http3Config {
    pub enabled         : bool,
    pub listen          : Option<SocketAddr>,
    pub max_idle_ms     : u64,
    pub max_streams     : u32,
    pub alt_svc_max_age : u64,
    pub congestion      : Congestion,
    pub stream_window   : u64,
    pub send_window     : u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct CacheConfig {
    pub enabled          : bool,
    pub capacity_bytes   : u64,
    pub items            : usize,
    pub max_object_bytes : u64,
    pub valid_ms         : BTreeMap<String, u64>,
    pub stale_ms         : u64,
    pub stale_if_error   : bool,
    pub key_headers      : Vec<String>,
    pub ignore_headers   : Vec<String>,
    pub lock             : bool,
    pub lock_ms          : u64,
    pub path             : Option<PathBuf>,
    pub disk_bytes       : u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct FilesConfig {
    pub cache_bytes    : u64,
    pub cache_items    : usize,
    pub max_file_bytes : u64,
    pub valid_ms       : u64,
    pub precompressed  : bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct CompressionConfig {
    pub enabled      : bool,
    pub min_bytes    : u64,
    pub level        : u32,
    pub brotli       : bool,
    pub brotli_level : u32,
    pub zstd         : bool,
    pub zstd_level   : u32,
    pub gunzip       : bool,
    pub types        : Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Every {
    #[default]
    Never,
    Hourly,
    Daily,
    Weekly,
    Monthly,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct AccessConfig {
    pub path            : PathBuf,
    pub format          : AccessFormat,
    pub pattern         : String,
    pub flush_ms        : u64,
    pub buffer          : usize,
    pub min_status      : u16,
    pub rotate_bytes    : u64,
    pub rotate_every    : Every,
    pub rotate_keep     : usize,
    pub rotate_compress : bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct DecisionConfig {
    pub enabled     : bool,
    pub path        : PathBuf,
    pub deny_ttl_ms : u64,
    pub capacity    : usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ModelSpec {
    pub dir      : PathBuf,
    pub features : Option<PathBuf>,
    pub threads  : usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct TlsConfig {
    pub cert                 : PathBuf,
    pub key                  : PathBuf,
    pub handshake_timeout_ms : u64,
    pub certificates         : Vec<Certificate>,
    pub session_cache        : usize,
    pub tickets              : bool,
    pub client_ca            : Option<PathBuf>,
    pub client_auth          : ClientAuth,
    pub ocsp                 : Option<PathBuf>,
    pub acme                 : Option<AcmeConfig>,
    pub min_version          : String,
    pub internal             : bool,
    pub ca_dir               : PathBuf,
    pub leaf_days            : u32,
    pub on_demand            : Option<OnDemandConfig>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct OnDemandConfig {
    pub names    : Vec<String>,
    pub ask      : Option<String>,
    pub capacity : usize,
    pub per_hour : u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct AcmeConfig {
    pub domains      : Vec<String>,
    pub email        : Option<String>,
    pub directory    : String,
    pub directory_ca : Option<PathBuf>,
    pub cache_dir    : PathBuf,
    pub challenge    : AcmeChallenge,
    pub listen       : Option<SocketAddr>,
    pub renew_days   : u64,
    pub dns_hook     : Option<PathBuf>,
    pub dns_wait_ms  : u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AcmeChallenge {
    #[default]
    #[serde(rename = "tls_alpn_01")]
    TlsAlpn01,
    #[serde(rename = "http_01")]
    Http01,
    #[serde(rename = "dns_01")]
    Dns01,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ClientAuth {
    #[default]
    Off,
    Optional,
    Required,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Certificate {
    pub names : Vec<String>,
    pub cert  : PathBuf,
    pub key   : PathBuf,
    pub ocsp  : Option<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct BackendConfig {
    pub address       : Address,
    pub weight        : u32,
    pub backup        : bool,
    pub down          : bool,
    pub max_in_flight : u64,
    pub tls           : bool,
    pub server_name   : String,
    pub ca_file       : Option<PathBuf>,
    pub cert          : Option<PathBuf>,
    pub key           : Option<PathBuf>,
    pub protocol      : Protocol,
    pub srv           : Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Balance {
    #[default]
    RoundRobin,
    LeastConn,
    LeastTime,
    Random,
    First,
    IpHash,
    Hash,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct StickyConfig {
    pub cookie    : String,
    pub ttl_ms    : u64,
    pub path      : String,
    pub secure    : bool,
    pub http_only : bool,
    pub same_site : Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct HealthConfig {
    pub interval_ms : u64,
    pub timeout_ms  : u64,
    pub path        : Option<String>,
    pub status      : u16,
    pub body        : Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct PoolOptions {
    pub policy               : Balance,
    pub hash_key             : Option<String>,
    pub sticky               : Option<StickyConfig>,
    pub max_fails            : u32,
    pub cooldown_ms          : u64,
    pub slow_start_ms        : u64,
    pub max_ejected          : u32,
    pub attempts             : usize,
    pub keepalive            : usize,
    pub slow_ms              : u64,
    pub retry_on             : Vec<String>,
    pub retry_non_idempotent : bool,
    pub health               : Option<HealthConfig>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PoolConfig {
    pub backends : Vec<BackendConfig>,
    pub options  : PoolOptions,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Route {
    pub name             : String,
    pub host             : Option<String>,
    pub path             : String,
    pub exact            : bool,
    pub regex            : Option<String>,
    pub prefer           : bool,
    pub methods          : Vec<String>,
    pub match_headers    : BTreeMap<String, String>,
    pub match_query      : BTreeMap<String, String>,
    pub match_vars       : BTreeMap<String, String>,
    pub internal         : bool,
    pub replace          : BTreeMap<String, String>,
    pub replace_types    : Vec<String>,
    pub upstream         : String,
    pub mirror           : Option<String>,
    pub strip_prefix     : bool,
    pub deny             : bool,
    pub capture          : bool,
    pub buffer_request   : Option<bool>,
    pub decisions        : Option<bool>,
    pub rate_limit_10s   : Option<u32>,
    pub rate_per_second  : Option<u32>,
    pub rate_burst       : Option<u32>,
    pub rate_key         : Option<String>,
    pub rate_rules       : Vec<RateRule>,
    pub concurrency_limit: Option<u32>,
    pub bandwidth        : Option<u64>,
    pub bandwidth_after  : Option<u64>,
    pub basic_auth       : Option<BasicAuth>,
    pub jwt              : Option<String>,
    pub forward_auth     : Option<ForwardAuth>,
    pub timeout_ms       : Option<u64>,
    pub max_body_bytes   : Option<usize>,
    pub preserve_host    : Option<bool>,
    pub request_headers  : BTreeMap<String, String>,
    pub response_headers : BTreeMap<String, String>,
    #[serde(deserialize_with = "one_or_many")]
    pub rewrite          : Vec<RewriteRule>,
    pub root             : Option<PathBuf>,
    pub index            : Option<String>,
    pub cache_control    : Option<String>,
    pub compress         : Option<bool>,
    pub gunzip           : Option<bool>,
    pub autoindex        : bool,
    pub buffer_response  : Option<bool>,
    pub cache            : Option<bool>,
    pub redirects        : Option<Vec<Replacement>>,
    pub cookie_domain    : Vec<Replacement>,
    pub cookie_path      : Vec<Replacement>,
    pub try_files        : Vec<String>,
    pub error_pages      : Vec<ErrorPage>,
    pub intercept_errors : bool,
    pub acl              : Acl,
    pub respond          : Option<Respond>,
    pub method           : Option<String>,
    pub abort            : bool,
    pub scheme           : Option<String>,
    pub satisfy_any      : bool,
    pub secure_link      : Option<SecureLink>,
    pub cache_bypass     : Vec<String>,
    pub log              : Option<bool>,
    pub charset          : Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Replacement {
    pub from : String,
    pub to   : String,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct JwtConfig {
    pub secret     : Option<String>,
    pub jwks       : Option<PathBuf>,
    pub algorithms : Vec<String>,
    pub issuer     : Option<String>,
    pub audience   : Option<String>,
    pub leeway_s   : u64,
    pub header     : Option<String>,
    pub cookie     : Option<String>,
    pub claims     : BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct SecureLink {
    pub secret     : Option<String>,
    pub secret_env : Option<String>,
    pub signature  : String,
    pub expires    : String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct BasicAuth {
    pub realm      : String,
    pub users      : BTreeMap<String, String>,
    pub users_file : Option<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ForwardAuth {
    pub upstream      : String,
    pub path          : String,
    pub copy_headers  : Vec<String>,
    pub timeout_ms    : u64,
    pub preserve_host : bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ErrorPage {
    #[serde(deserialize_with = "one_or_many")]
    pub status : Vec<u16>,
    pub page   : String,
    pub code   : Option<u16>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct RewriteRule {
    pub from   : String,
    pub to     : String,
    pub status : Option<u16>,
}

fn nets <'de, D> ( deserializer: D ) -> Result<Vec<IpNet>, D::Error> where D: serde::Deserializer<'de> {

    Vec::<String>::deserialize(deserializer)?.iter().map(|text| text.parse::<IpNet>().or_else(|_| text.parse::<std::net::IpAddr>().map(IpNet::from)).map_err(|_| serde::de::Error::custom(format!("`{text}` is not an address or a network")))).collect()

}

fn one_or_many <'de, D, T> ( deserializer: D ) -> Result<Vec<T>, D::Error> where D: serde::Deserializer<'de>, T: Deserialize<'de> {

    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Shape <T> { Many(Vec<T>), One(T) }

    Ok(match Shape::<T>::deserialize(deserializer)? { Shape::Many(items) => items, Shape::One(item) => vec![item] })

}
