use std::collections::BTreeMap;
use std::net::SocketAddr;
use std::path::PathBuf;

use crate::core::net::Address;
use crate::config::base::consts::{
    ACCESS_BUFFER, ACCESS_FLUSH_MS, ACME_CACHE_DIR, STREAM_IDLE_MS, ACME_DIRECTORY, ACME_RENEW_DAYS, RESOLVE_MS, ZSTD_LEVEL, H2_CONNECTION_WINDOW, H2_MAX_FRAME, H2_MAX_HEADER_BYTES, H2_STREAM_WINDOW, HTTP3_SEND_WINDOW, HTTP3_STREAM_WINDOW, TLS_SESSION_CACHE, CACHE_BYTES, HTTP3_ALT_SVC_MAX_AGE, HTTP3_IDLE_MS, CACHE_DISK_BYTES, CACHE_ITEMS, CACHE_LOCK_MS, CACHE_OBJECT_BYTES, MAX_EJECTED, FILES_CACHE_BYTES, FILES_CACHE_ITEMS, FILES_MAX_BYTES, FILES_PRECOMPRESSED, FILES_VALID_MS, RESPONSE_BUFFER_BYTES, STICKY_COOKIE, BROTLI_LEVEL, COMPRESSION_LEVEL, COMPRESSION_MIN_BYTES, COMPRESSION_TYPES, ANALYSIS_CAPACITY, ANALYSIS_DEADLINE_MS, ANALYSIS_MODEL, ANALYSIS_WORKERS, ATTEMPTS, BACKLOG, BUFFER_REQUESTS, CLIENT_TIMEOUT_MS, CLIENT_BUF, CONNECT_ATTEMPTS,
    CONNECT_TIMEOUT_MS, CONTROL_BODY_BYTES, CONTROL_LISTEN, CONTROL_PREFIX, DECISIONS_CAPACITY, DECISIONS_PATH, DENY_TTL_MS,
    CONTROL_TOKEN_ENV, COOLDOWN_MS, DRAIN_MS, FORWARDED_FOR_HEADER,
    FORWARDED_PROTO_HEADER, FORWARDING, HEADER_TIMEOUT_MS, KEEPALIVE_REQUESTS, KEEPALIVE_TIME_MS, MAX_CONNECTIONS, KEEPALIVE_TIMEOUT_MS, SEND_TIMEOUT_MS, UNDERSCORES, HTTP2, HEALTH_INTERVAL_MS, HEALTH_STATUS, HEALTH_TIMEOUT_MS, KEEPALIVE, LISTEN,
    JOURNEYS, LOG_JSON, LOG_LEVEL, MAX_BODY_BYTES, MAX_FAILS, MAX_HEADERS, MAX_IN_FLIGHT, MAX_STREAMS, PIN, POOL_CAPACITY, POOL_IDLE_MS, PROPAGATE, PROXY_PROTOCOL,
    RATE_BURST, RATE_LIMIT_10S, RATE_PER_SECOND, RECENT_EVENTS, REQUEST_ID_HEADER, RESPOND_TYPE, RETRY_ON, SCAN_BYTES, SERVER_BUF, H2C, TIMEOUT_MS, TLS_HANDSHAKE_TIMEOUT_MS, WEIGHT, WORKERS_AUTO,
};
use crate::core::net::Addr;
use crate::http::h3::Congestion;
use crate::http::server::Accept;
use crate::http::upstream::Protocol;
use super::arch::{Acl, HookSpec, ListenConfig, Respond, SecureLink, AccessConfig, AccessFormat, Every, OnDemandConfig, AcmeChallenge, AcmeConfig, AnalysisConfig, AnalysisMode, BackendConfig, Balance, BasicAuth, CacheConfig, ClientAuth, ClientConfig, CompressionConfig, Config, ControlConfig, DecisionConfig, FilesConfig, ForwardAuth, HealthConfig, Http3Config, IdentityConfig, Limits, LogConfig, ModelSpec, PoolOptions, Route, RuntimeConfig, ServerConfig, StickyConfig, StreamConfig, TelemetryConfig, TlsConfig};

impl Default for Config {

    fn default () -> Self {

        Self {
            listen_unix      : None,
            listen           : Addr::parse(LISTEN).unwrap_or_else(|_| ([127, 0, 0, 1], 8080).into()),
            tls              : None,
            runtime          : RuntimeConfig::default(),
            server           : ServerConfig::default(),
            client           : ClientConfig::default(),
            limits           : Limits::default(),
            identity         : IdentityConfig::default(),
            log              : LogConfig::default(),
            control          : ControlConfig::default(),
            telemetry        : TelemetryConfig::default(),
            models           : BTreeMap::new(),
            analysis         : AnalysisConfig::default(),
            decisions        : DecisionConfig::default(),
            access           : AccessConfig::default(),
            compression      : CompressionConfig::default(),
            files            : FilesConfig::default(),
            cache            : CacheConfig::default(),
            http3            : Http3Config::default(),
            default_pool     : None,
            pools            : BTreeMap::new(),
            routes           : Vec::new(),
            request_headers  : BTreeMap::new(),
            response_headers : BTreeMap::new(),
            preserve_host    : false,
            error_pages      : Vec::new(),
            streams          : Vec::new(),
            variables        : BTreeMap::new(),
            jwts             : BTreeMap::new(),
            listeners        : Vec::new(),
            acl              : Acl::default(),
            hooks            : HookSpec::default(),
        }

    }

}

impl Default for RuntimeConfig {

    fn default () -> Self {

        Self { workers: WORKERS_AUTO, pin: PIN, backlog: BACKLOG, accept: Accept::Auto }

    }

}

impl Default for ServerConfig {

    fn default () -> Self {

        Self {
            keepalive         : KEEPALIVE,
            max_headers       : MAX_HEADERS,
            max_connections   : MAX_CONNECTIONS,
            header_timeout_ms : HEADER_TIMEOUT_MS,
            keepalive_timeout_ms : KEEPALIVE_TIMEOUT_MS,
            keepalive_requests : KEEPALIVE_REQUESTS,
            send_timeout_ms   : SEND_TIMEOUT_MS,
            keepalive_time_ms : KEEPALIVE_TIME_MS,
            underscores_in_headers : UNDERSCORES,
            buffer            : SERVER_BUF,
            drain_ms          : DRAIN_MS,
            http2             : HTTP2,
            h2c               : H2C,
            proxy_protocol    : PROXY_PROTOCOL,
            max_streams       : MAX_STREAMS,
            h2_adaptive_window: false,
            h2_stream_window  : H2_STREAM_WINDOW,
            h2_connection_window : H2_CONNECTION_WINDOW,
            h2_max_frame      : H2_MAX_FRAME,
            h2_max_header_bytes : H2_MAX_HEADER_BYTES,
        }

    }

}

impl Default for ClientConfig {

    fn default () -> Self {

        Self {
            connect_timeout_ms : CONNECT_TIMEOUT_MS,
            pool_idle_ms       : POOL_IDLE_MS,
            pool_capacity      : POOL_CAPACITY,
            buffer             : CLIENT_BUF,
            resolve_ms         : RESOLVE_MS,
            attempts           : CONNECT_ATTEMPTS,
        }

    }

}

impl Default for Limits {

    fn default () -> Self {

        Self { timeout_ms: TIMEOUT_MS, client_timeout_ms: CLIENT_TIMEOUT_MS, max_body_bytes: MAX_BODY_BYTES, max_in_flight: MAX_IN_FLIGHT, rate_limit_10s: RATE_LIMIT_10S, rate_per_second: RATE_PER_SECOND, rate_burst: RATE_BURST, rate_key: None, rate_rules: Vec::new(), concurrency_limit: 0, bandwidth: 0, bandwidth_after: 0, buffer_requests: BUFFER_REQUESTS, spool_bytes: 262_144, spool_dir: PathBuf::new(), buffer_responses: false, response_buffer_bytes: RESPONSE_BUFFER_BYTES }

    }

}

impl Default for IdentityConfig {

    fn default () -> Self {

        Self {
            request_id_header      : REQUEST_ID_HEADER.to_string(),
            propagate              : PROPAGATE,
            forwarding             : FORWARDING,
            forwarded_for_header   : FORWARDED_FOR_HEADER.to_string(),
            forwarded_proto_header : FORWARDED_PROTO_HEADER.to_string(),
            trusted_peers          : Vec::new(),
            actor_header           : None,
            backend_block_header   : None,
            traceparent            : false,
        }

    }

}

impl Default for ControlConfig {

    fn default () -> Self {

        Self {
            enabled           : false,
            listen            : Addr::parse(CONTROL_LISTEN).unwrap_or_else(|_| ([127, 0, 0, 1], 9090).into()),
            prefix            : CONTROL_PREFIX.to_string(),
            token_env         : CONTROL_TOKEN_ENV.to_string(),
            backend_token_env : None,
            panel             : false,
            panel_dir         : None,
            body_bytes        : CONTROL_BODY_BYTES,
        }

    }

}

impl Default for TelemetryConfig {

    fn default () -> Self {

        Self { enabled: true, recent: RECENT_EVENTS, journeys: JOURNEYS, otlp: None, otlp_interval_ms: 5_000, otlp_headers: BTreeMap::new() }

    }

}

impl Default for AnalysisConfig {

    fn default () -> Self {

        Self {
            mode                : AnalysisMode::Off,
            model               : ANALYSIS_MODEL.to_string(),
            scan_bytes          : SCAN_BYTES,
            response_scan_bytes : SCAN_BYTES,
            workers             : ANALYSIS_WORKERS,
            capacity            : ANALYSIS_CAPACITY,
            deadline_ms         : ANALYSIS_DEADLINE_MS,
        }

    }

}

impl Default for Http3Config {

    fn default () -> Self {

        Self { enabled: false, listen: None, max_idle_ms: HTTP3_IDLE_MS, max_streams: MAX_STREAMS, alt_svc_max_age: HTTP3_ALT_SVC_MAX_AGE, congestion: Congestion::default(), stream_window: HTTP3_STREAM_WINDOW, send_window: HTTP3_SEND_WINDOW }

    }

}

impl Default for CacheConfig {

    fn default () -> Self {

        Self { enabled: false, capacity_bytes: CACHE_BYTES, items: CACHE_ITEMS, max_object_bytes: CACHE_OBJECT_BYTES, valid_ms: BTreeMap::new(), stale_ms: 0, stale_if_error: false, key_headers: Vec::new(), ignore_headers: Vec::new(), lock: true, lock_ms: CACHE_LOCK_MS, path: None, disk_bytes: CACHE_DISK_BYTES }

    }

}

impl Default for FilesConfig {

    fn default () -> Self {

        Self { cache_bytes: FILES_CACHE_BYTES, cache_items: FILES_CACHE_ITEMS, max_file_bytes: FILES_MAX_BYTES, valid_ms: FILES_VALID_MS, precompressed: FILES_PRECOMPRESSED }

    }

}

impl Default for CompressionConfig {

    fn default () -> Self {

        Self { enabled: false, min_bytes: COMPRESSION_MIN_BYTES, level: COMPRESSION_LEVEL, brotli: true, brotli_level: BROTLI_LEVEL, zstd: true, zstd_level: ZSTD_LEVEL, gunzip: false, types: COMPRESSION_TYPES.iter().map(|kind| (*kind).to_string()).collect() }

    }

}

impl Default for AccessConfig {

    fn default () -> Self {

        Self { path: PathBuf::new(), format: AccessFormat::Combined, pattern: String::new(), flush_ms: ACCESS_FLUSH_MS, buffer: ACCESS_BUFFER, min_status: 0, rotate_bytes: 0, rotate_every: Every::Never, rotate_keep: 10, rotate_compress: false }

    }

}

impl Default for DecisionConfig {

    fn default () -> Self {

        Self { enabled: false, path: PathBuf::from(DECISIONS_PATH), deny_ttl_ms: DENY_TTL_MS, capacity: DECISIONS_CAPACITY }

    }

}

impl Default for ModelSpec {

    fn default () -> Self {

        Self { dir: PathBuf::new(), features: None, threads: 1 }

    }

}

impl Default for SecureLink {

    fn default () -> Self {

        Self { secret: None, secret_env: None, signature: "sig".to_string(), expires: "expires".to_string() }

    }

}

impl Default for OnDemandConfig {

    fn default () -> Self {

        Self { names: Vec::new(), ask: None, capacity: 1_000, per_hour: 60 }

    }

}

impl Default for TlsConfig {

    fn default () -> Self {

        Self { cert: PathBuf::new(), key: PathBuf::new(), handshake_timeout_ms: TLS_HANDSHAKE_TIMEOUT_MS, certificates: Vec::new(), session_cache: TLS_SESSION_CACHE, tickets: true, client_ca: None, client_auth: ClientAuth::Off, ocsp: None, acme: None, min_version: "1.2".to_string(), internal: false, ca_dir: PathBuf::from("ca"), leaf_days: 7, on_demand: None }

    }

}

impl Default for LogConfig {

    fn default () -> Self {

        Self { level: LOG_LEVEL.to_string(), json: LOG_JSON }

    }

}

impl Default for BackendConfig {

    fn default () -> Self {

        Self {
            address       : Address::Tcp(([127, 0, 0, 1], 3000).into()),
            backup        : false,
            down          : false,
            weight        : WEIGHT,
            max_in_flight : 0,
            tls           : false,
            server_name   : String::new(),
            ca_file       : None,
            cert          : None,
            key           : None,
            protocol      : Protocol::Auto,
            srv           : None,
        }

    }

}

impl Default for HealthConfig {

    fn default () -> Self {

        Self { interval_ms: HEALTH_INTERVAL_MS, timeout_ms: HEALTH_TIMEOUT_MS, path: None, status: HEALTH_STATUS, body: None }

    }

}

impl Default for PoolOptions {

    fn default () -> Self {

        Self { policy: Balance::default(), hash_key: None, sticky: None, max_fails: MAX_FAILS, cooldown_ms: COOLDOWN_MS, slow_start_ms: 0, max_ejected: MAX_EJECTED, attempts: ATTEMPTS, keepalive: 0, slow_ms: 0, retry_on: RETRY_ON.iter().map(|token| token.to_string()).collect(), retry_non_idempotent: false, health: None }

    }

}

impl Default for StickyConfig {

    fn default () -> Self {

        Self { cookie: STICKY_COOKIE.to_string(), ttl_ms: 0, path: "/".to_string(), secure: false, http_only: true, same_site: Some("Lax".to_string()) }

    }

}

impl Default for Route {

    fn default () -> Self {

        Self {
            name             : String::new(),
            host             : None,
            path             : "/".to_string(),
            exact            : false,
            regex            : None,
            prefer           : false,
            methods          : Vec::new(),
            match_headers    : BTreeMap::new(),
            match_query      : BTreeMap::new(),
            match_vars       : BTreeMap::new(),
            internal         : false,
            replace          : BTreeMap::new(),
            replace_types    : vec!["text/html".to_string()],
            upstream         : String::new(),
            mirror           : None,
            strip_prefix     : false,
            deny             : false,
            capture          : false,
            buffer_request   : None,
            decisions        : None,
            rate_limit_10s   : None,
            rate_per_second  : None,
            rate_burst       : None,
            rate_key         : None,
            rate_rules       : Vec::new(),
            concurrency_limit: None,
            bandwidth        : None,
            bandwidth_after  : None,
            basic_auth       : None,
            jwt              : None,
            forward_auth     : None,
            timeout_ms       : None,
            max_body_bytes   : None,
            preserve_host    : None,
            request_headers  : BTreeMap::new(),
            response_headers : BTreeMap::new(),
            rewrite          : Vec::new(),
            root             : None,
            index            : None,
            cache_control    : None,
            compress         : None,
            gunzip           : None,
            autoindex        : false,
            buffer_response  : None,
            cache            : None,
            redirects        : None,
            cookie_domain    : Vec::new(),
            cookie_path      : Vec::new(),
            try_files        : Vec::new(),
            error_pages      : Vec::new(),
            intercept_errors : false,
            acl              : Acl::default(),
            respond          : None,
            method           : None,
            scheme           : None,
            abort            : false,
            satisfy_any      : false,
            secure_link      : None,
            cache_bypass     : Vec::new(),
            log              : None,
            charset          : None,
        }

    }

}

impl Default for BasicAuth {

    fn default () -> Self {

        Self { realm: "restricted".to_string(), users: BTreeMap::new(), users_file: None }

    }

}

impl Default for ForwardAuth {

    fn default () -> Self {

        Self { upstream: String::new(), path: "/".to_string(), copy_headers: Vec::new(), timeout_ms: TIMEOUT_MS, preserve_host: false }

    }

}

impl Default for AcmeConfig {

    fn default () -> Self {

        Self { domains: Vec::new(), email: None, directory: ACME_DIRECTORY.to_string(), directory_ca: None, cache_dir: PathBuf::from(ACME_CACHE_DIR), challenge: AcmeChallenge::default(), listen: None, renew_days: ACME_RENEW_DAYS, dns_hook: None, dns_wait_ms: 30_000 }

    }

}

impl Default for ListenConfig {

    fn default () -> Self {

        Self { address: SocketAddr::from(( [0, 0, 0, 0], 0 )), tls: false, redirect: false, proxy_protocol: PROXY_PROTOCOL }

    }

}

impl Default for Respond {

    fn default () -> Self {

        Self { status: 200, body: String::new(), content_type: RESPOND_TYPE.to_string() }

    }

}

impl Default for StreamConfig {

    fn default () -> Self {

        Self { name: String::new(), listen: SocketAddr::from(( [0, 0, 0, 0], 0 )), upstream: String::new(), udp: false, idle_ms: STREAM_IDLE_MS, proxy_protocol: None, sni: BTreeMap::new(), acl: Acl::default(), max_connections: 0 }

    }

}
