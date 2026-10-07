#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Dsl;

pub const SETTERS: [&str; 18] = ["set_acl", "set_runtime", "set_server", "set_client", "set_limits", "set_identity", "set_log", "set_tls", "set_control", "set_telemetry", "set_analysis", "set_decisions", "set_access_log", "set_compression", "set_files", "set_cache", "set_http3", "set_error_pages"];

#[derive(Debug, Clone, Default, serde::Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Site {
    pub host          : Option<String>,
    pub path          : String,
    pub root          : std::path::PathBuf,
    pub fastcgi       : Option<String>,
    pub cache_control : Option<String>,
}
