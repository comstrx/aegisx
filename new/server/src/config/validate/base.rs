use std::collections::{BTreeMap, HashSet};

use http::header::HeaderName;

use crate::config::base::consts::{
    ACCESS_BUFFER_MAX, ACL_MAX, LISTENERS_MAX, RESPOND_BYTES_MAX, ANALYSIS_CAPACITY_MAX, ATTEMPTS_MAX, BACKENDS_MAX, BANDWIDTH_MAX, BUF_MAX, BUF_MIN, CONCURRENCY_MAX, CONNECT_ATTEMPTS_MAX, CONTROL_BODY_BYTES_MAX, COOLDOWN_MS_MAX, COOLDOWN_MS_MIN, DECISIONS_CAPACITY_MAX, DENY_TTL_MS_MAX, HEADERS_MAX, HEADER_VALUE_MAX, HEALTH_INTERVAL_MS_MAX, HEALTH_INTERVAL_MS_MIN, JOURNEYS_MAX, MAX_BODY_BYTES_MAX, MAX_CONNECTIONS_MAX, MAX_FAILS_MAX, MAX_HEADERS_MAX, MAX_IN_FLIGHT_MAX, MAX_STREAMS_MAX, POOLS_MAX, RATE_LIMIT_MAX, RECENT_EVENTS_MAX, ROUTES_MAX, SCAN_BYTES_MAX, SLOW_START_MS_MAX, TIMEOUT_MS_MAX, TIMEOUT_MS_MIN, WEIGHT_MAX, WORKERS_MAX,
};
use crate::config::spec::{AcmeChallenge, AnalysisMode, ClientAuth, Config, ErrorPage, Every, RateRule};
use crate::core::error::{AppError, AppResult};
use crate::core::jwt::Algorithm;
use crate::core::net::Address;
use crate::http::header::Var;
use crate::http::key::HashKey;
use crate::http::upstream::Protocol;
use crate::http::variable::Catalog;

impl Config {

    pub fn validate ( &self ) -> AppResult<()> {

        Self::range("runtime.workers", self.runtime.workers as u64, 0, WORKERS_MAX as u64)?;
        Self::range("runtime.backlog", self.runtime.backlog.max(0) as u64, 1, 1_048_576)?;
        Self::range("server.max_headers", self.server.max_headers as u64, 1, MAX_HEADERS_MAX as u64)?;
        Self::range("server.max_connections", self.server.max_connections as u64, 0, MAX_CONNECTIONS_MAX as u64)?;
        Self::range("server.header_timeout_ms", self.server.header_timeout_ms, TIMEOUT_MS_MIN, TIMEOUT_MS_MAX)?;
        Self::range("server.keepalive_timeout_ms", self.server.keepalive_timeout_ms, TIMEOUT_MS_MIN, TIMEOUT_MS_MAX)?;
        Self::range("server.send_timeout_ms", self.server.send_timeout_ms, TIMEOUT_MS_MIN, TIMEOUT_MS_MAX)?;
        Self::range("server.buffer", self.server.buffer as u64, BUF_MIN as u64, BUF_MAX as u64)?;
        Self::range("server.drain_ms", self.server.drain_ms, 0, TIMEOUT_MS_MAX)?;
        Self::range("server.max_streams", u64::from(self.server.max_streams), 1, u64::from(MAX_STREAMS_MAX))?;
        Self::range("server.h2_stream_window", u64::from(self.server.h2_stream_window), 65_535, 2_147_483_647)?;
        Self::range("server.h2_connection_window", u64::from(self.server.h2_connection_window), 65_535, 2_147_483_647)?;
        Self::range("server.h2_max_frame", u64::from(self.server.h2_max_frame), 16_384, 16_777_215)?;
        Self::range("server.h2_max_header_bytes", u64::from(self.server.h2_max_header_bytes), 4_096, 16_777_216)?;
        Self::range("client.connect_timeout_ms", self.client.connect_timeout_ms, TIMEOUT_MS_MIN, TIMEOUT_MS_MAX)?;
        Self::range("client.pool_idle_ms", self.client.pool_idle_ms, 1_000, TIMEOUT_MS_MAX)?;
        Self::range("client.pool_capacity", self.client.pool_capacity as u64, 1, 65_536)?;

        if self.client.resolve_ms != 0 { Self::range("client.resolve_ms", self.client.resolve_ms, 1_000, 86_400_000)?; }
        Self::range("client.buffer", self.client.buffer as u64, BUF_MIN as u64, BUF_MAX as u64)?;
        Self::range("client.attempts", self.client.attempts as u64, 1, CONNECT_ATTEMPTS_MAX as u64)?;
        Self::range("limits.timeout_ms", self.limits.timeout_ms, TIMEOUT_MS_MIN, TIMEOUT_MS_MAX)?;
        Self::range("limits.client_timeout_ms", self.limits.client_timeout_ms, TIMEOUT_MS_MIN, TIMEOUT_MS_MAX)?;
        Self::range("limits.max_body_bytes", self.limits.max_body_bytes as u64, 1, MAX_BODY_BYTES_MAX as u64)?;
        Self::range("limits.max_in_flight", self.limits.max_in_flight as u64, 1, MAX_IN_FLIGHT_MAX as u64)?;
        Self::range("limits.rate_limit_10s", u64::from(self.limits.rate_limit_10s), 0, u64::from(RATE_LIMIT_MAX))?;
        Self::range("limits.rate_per_second", u64::from(self.limits.rate_per_second), 0, u64::from(RATE_LIMIT_MAX))?;
        Self::range("limits.rate_burst", u64::from(self.limits.rate_burst), 0, u64::from(RATE_LIMIT_MAX))?;
        Self::range("limits.concurrency_limit", u64::from(self.limits.concurrency_limit), 0, u64::from(CONCURRENCY_MAX))?;
        Self::rules("limits", &self.limits.rate_rules)?;
        Self::range("limits.bandwidth", self.limits.bandwidth, 0, BANDWIDTH_MAX)?;
        Self::range("limits.bandwidth_after", self.limits.bandwidth_after, 0, BANDWIDTH_MAX)?;

        if let Some(tls) = &self.tls {

            if tls.internal && tls.acme.is_some() { return Err(AppError::config("set_tls", "internal and acme are two issuers; set one")); }

            Self::range("tls.leaf_days", u64::from(tls.leaf_days), 1, 397)?;

            if !matches!(tls.min_version.as_str(), "1.2" | "1.3") { return Err(AppError::config("set_tls", "min_version is 1.2 or 1.3")); }

            if let Some(plan) = &tls.on_demand {

                if !tls.internal && tls.acme.is_none() { return Err(AppError::config("set_tls", "on_demand needs an issuer: internal = true or acme")); }

                if tls.acme.is_some() && plan.names.is_empty() && plan.ask.is_none() { return Err(AppError::config("set_tls", "on_demand with acme needs names or ask; issuing for any name a client sends is refused")); }

                if plan.ask.as_ref().is_some_and(|ask| tls.internal || !ask.starts_with("http://")) { return Err(AppError::config("set_tls", "on_demand ask is an http:// url and belongs to acme")); }

                if plan.names.iter().any(|name| name.is_empty() || name.contains(char::is_whitespace)) { return Err(AppError::config("set_tls", "on_demand names must be host names, `*.suffix` or `*`")); }

                Self::range("tls.on_demand.capacity", plan.capacity as u64, 1, 100_000)?;
                Self::range("tls.on_demand.per_hour", u64::from(plan.per_hour), 1, 10_000)?;

            }

            match &tls.acme {
                None if tls.internal => {}
                None => {

                    if tls.cert.as_os_str().is_empty() { return Err(AppError::config("set_tls", "cert is required")); }

                    if tls.key.as_os_str().is_empty() { return Err(AppError::config("set_tls", "key is required")); }

                }
                Some(acme) => {

                    if acme.domains.is_empty() && tls.on_demand.is_none() { return Err(AppError::config("set_tls", "acme needs at least one domain or on_demand")); }

                    for domain in &acme.domains { if Address::parse(&format!("{domain}:443")).ok().and_then(|address| address.name().map(|( host, _ )| host.to_owned())).is_none_or(|host| host.starts_with("*.") && acme.challenge != AcmeChallenge::Dns01) { return Err(AppError::config("set_tls", format!("acme domain `{domain}` is not a valid host name; wildcards need challenge dns_01"))); } }

                    if (acme.challenge == AcmeChallenge::Dns01) != acme.dns_hook.is_some() { return Err(AppError::config("set_tls", "acme challenge dns_01 and dns_hook go together")); }

                    Self::range("tls.acme.dns_wait_ms", acme.dns_wait_ms, 0, TIMEOUT_MS_MAX)?;

                    if !(acme.directory.starts_with("https://") || acme.directory == "staging" || acme.directory == "production") { return Err(AppError::config("set_tls", "acme directory must be an https url, staging or production")); }

                    if acme.cache_dir.as_os_str().is_empty() { return Err(AppError::config("set_tls", "acme cache_dir is required")); }

                    Self::range("tls.acme.renew_days", acme.renew_days, 1, 89)?;

                }
            }


            Self::range("tls.handshake_timeout_ms", tls.handshake_timeout_ms, TIMEOUT_MS_MIN, TIMEOUT_MS_MAX)?;
            Self::range("tls.session_cache", tls.session_cache as u64, 0, 4_194_304)?;

            if tls.client_auth != ClientAuth::Off && tls.client_ca.is_none() { return Err(AppError::config("set_tls", "client_auth needs client_ca")); }

            for certificate in &tls.certificates {

                if certificate.names.is_empty() { return Err(AppError::config("add_certificate", "names must list at least one server name")); }

                if certificate.cert.as_os_str().is_empty() || certificate.key.as_os_str().is_empty() { return Err(AppError::config("add_certificate", "cert and key are required")); }

                for name in &certificate.names {

                    let bare = name.strip_prefix("*.").unwrap_or(name);

                    if bare.is_empty() || bare.contains('*') || !bare.bytes().all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'.') || bare.starts_with('.') || bare.ends_with('.') {

                        return Err(AppError::config("add_certificate", format!("invalid server name `{name}`")));

                    }

                }

            }

        }

        for ( name, spec ) in &self.models {

            if name.is_empty() { return Err(AppError::config("add_model", "model name must not be empty")); }

            if spec.dir.as_os_str().is_empty() { return Err(AppError::config("add_model", format!("model `{name}` needs a dir"))); }

            Self::range(&format!("model `{name}` threads"), spec.threads as u64, 1, 64)?;

        }

        if self.analysis.mode != AnalysisMode::Off {

            if !self.models.contains_key(&self.analysis.model) { return Err(AppError::config("set_analysis", format!("model `{}` is not declared with add_model", self.analysis.model))); }

            Self::range("analysis.scan_bytes", self.analysis.scan_bytes as u64, 0, SCAN_BYTES_MAX as u64)?;
            Self::range("analysis.response_scan_bytes", self.analysis.response_scan_bytes as u64, 0, SCAN_BYTES_MAX as u64)?;
            Self::range("analysis.workers", self.analysis.workers as u64, 1, 64)?;
            Self::range("analysis.capacity", self.analysis.capacity as u64, 1, ANALYSIS_CAPACITY_MAX as u64)?;
            Self::range("analysis.deadline_ms", self.analysis.deadline_ms, TIMEOUT_MS_MIN, TIMEOUT_MS_MAX)?;

        }

        Self::range("telemetry.recent", self.telemetry.recent as u64, 1, RECENT_EVENTS_MAX as u64)?;
        Self::range("telemetry.journeys", self.telemetry.journeys as u64, 1, JOURNEYS_MAX as u64)?;

        if self.control.enabled {

            if !self.control.listen.ip().is_loopback() { return Err(AppError::config("set_control", "listen must be a loopback address")); }

            if !self.control.prefix.starts_with('/') || self.control.prefix.ends_with('/') { return Err(AppError::config("set_control", "prefix must start with `/` and not end with `/`")); }

            if self.control.token_env.is_empty() { return Err(AppError::config("set_control", "token_env is required")); }

            if self.control.backend_token_env.as_deref() == Some(self.control.token_env.as_str()) { return Err(AppError::config("set_control", "backend_token_env must differ from token_env")); }

            if self.control.panel && self.control.panel_dir.is_none() { return Err(AppError::config("set_control", "panel_dir is required when panel is enabled")); }

            Self::range("control.body_bytes", self.control.body_bytes as u64, 256, CONTROL_BODY_BYTES_MAX as u64)?;

        }

        Self::header_name("identity.request_id_header", &self.identity.request_id_header)?;
        Self::header_name("identity.forwarded_for_header", &self.identity.forwarded_for_header)?;
        Self::header_name("identity.forwarded_proto_header", &self.identity.forwarded_proto_header)?;

        if let Some(name) = &self.identity.actor_header { Self::header_name("identity.actor_header", name)?; }

        if let Some(name) = &self.identity.backend_block_header { Self::header_name("identity.backend_block_header", name)?; }

        if self.http3.enabled && self.tls.is_none() { return Err(AppError::config("set_http3", "http/3 needs set_tls")); }

        if self.http3.enabled {

            Self::range("http3.max_idle_ms", self.http3.max_idle_ms, 1_000, TIMEOUT_MS_MAX)?;
            Self::range("http3.stream_window", self.http3.stream_window, 65_535, 1_073_741_824)?;
            Self::range("http3.send_window", self.http3.send_window, 65_535, 4_294_967_296)?;

        }

        for name in self.variables.keys() {

            if name.is_empty() || !name.bytes().all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_') || name.as_bytes()[0].is_ascii_digit() || Var::named(name).is_some() {

                return Err(AppError::config("variable", format!("`{name}` must be lowercase letters, digits and underscores and cannot shadow a built-in variable")));

            }

        }

        Catalog::compile(&self.variables)?;

        if let Some(target) = &self.telemetry.otlp && Address::parse(target.trim_start_matches("http://").trim_end_matches('/')).is_err() { return Err(AppError::config("set_telemetry", format!("otlp `{target}` must be host:port or http://host:port"))); }

        Self::range("telemetry.otlp_interval_ms", self.telemetry.otlp_interval_ms, 100, TIMEOUT_MS_MAX)?;
        Self::headers("set_telemetry.otlp_headers", &self.telemetry.otlp_headers)?;

        for ( name, jwt ) in &self.jwts {

            if jwt.secret.as_deref().is_none_or(str::is_empty) == jwt.jwks.is_none() { return Err(AppError::config("add_jwt", format!("`{name}` needs exactly one of secret or jwks"))); }

            if let Some(unknown) = jwt.algorithms.iter().find(|algorithm| Algorithm::named(algorithm).is_none()) { return Err(AppError::config("add_jwt", format!("`{name}` does not know algorithm `{unknown}`"))); }

            for header in jwt.claims.values().chain(jwt.header.iter()) { Self::header_name(&format!("add_jwt `{name}`"), header)?; }

        }

        for name in &self.cache.key_headers { Self::header_name("set_cache.key_headers", name)?; }

        for name in &self.cache.ignore_headers { if !matches!(name.to_ascii_lowercase().as_str(), "cache-control" | "expires" | "set-cookie" | "vary") { return Err(AppError::config("set_cache", format!("ignore_headers entry `{name}` must be one of cache-control, expires, set-cookie, vary"))); } }

        for status in self.cache.valid_ms.keys() { if status != "any" && status.parse::<u16>().is_err() { return Err(AppError::config("set_cache", format!("valid_ms key `{status}` must be a status code or any"))); } }

        Self::range("cache.max_object_bytes", self.cache.max_object_bytes, 1_024, self.cache.capacity_bytes.max(1_024))?;
        Self::range("cache.lock_ms", self.cache.lock_ms, 0, TIMEOUT_MS_MAX)?;
        Self::range("cache.disk_bytes", self.cache.disk_bytes, self.cache.max_object_bytes, 1 << 50)?;
        Self::range("limits.response_buffer_bytes", self.limits.response_buffer_bytes as u64, 1_024, MAX_BODY_BYTES_MAX as u64)?;
        Self::range("limits.spool_bytes", self.limits.spool_bytes as u64, 1_024, MAX_BODY_BYTES_MAX as u64)?;
        Self::range("compression.level", u64::from(self.compression.level), 1, 9)?;
        Self::range("compression.brotli_level", u64::from(self.compression.brotli_level), 0, 11)?;
        Self::range("compression.zstd_level", u64::from(self.compression.zstd_level), 1, 19)?;
        Self::range("compression.min_bytes", self.compression.min_bytes, 0, MAX_BODY_BYTES_MAX as u64)?;

        if self.compression.types.is_empty() { return Err(AppError::config("set_compression", "types must not be empty")); }

        if !self.access.path.as_os_str().is_empty() {

            Self::range("access.flush_ms", self.access.flush_ms, 10, TIMEOUT_MS_MAX)?;
            Self::range("access.buffer", self.access.buffer as u64, 1_024, ACCESS_BUFFER_MAX as u64)?;
            Self::range("access.rotate_keep", self.access.rotate_keep as u64, 1, 1_000)?;

            let rotating = self.access.rotate_bytes > 0 || self.access.rotate_every != Every::Never;
            let target = self.access.path.to_string_lossy();

            if self.access.rotate_bytes > 0 && self.access.rotate_every != Every::Never { return Err(AppError::config("set_access_log", "rotate_bytes and rotate_every are two ways to rotate; set one")); }

            if rotating && (target == "stdout" || target.contains("://")) { return Err(AppError::config("set_access_log", "rotation needs a file path")); }

            if self.access.rotate_bytes > 0 { Self::range("access.rotate_bytes", self.access.rotate_bytes, 4_096, u64::MAX)?; }

        }

        if self.decisions.enabled {

            if self.decisions.path.as_os_str().is_empty() { return Err(AppError::config("set_decisions", "path is required")); }

            Self::range("decisions.deny_ttl_ms", self.decisions.deny_ttl_ms, 1_000, DENY_TTL_MS_MAX)?;
            Self::range("decisions.capacity", self.decisions.capacity as u64, 1, DECISIONS_CAPACITY_MAX as u64)?;

        }

        Self::headers("set_headers.request", &self.request_headers)?;
        Self::headers("set_headers.response", &self.response_headers)?;

        if self.pools.len() > POOLS_MAX { return Err(AppError::config("add_upstream", format!("more than {POOLS_MAX} pools"))); }

        let mut backends = 0;

        for ( name, pool ) in &self.pools {

            if let Some(key) = &pool.options.hash_key && !matches!(key.split_once(':').map_or(key.as_str(), |( kind, _ )| kind), "ip" | "uri" | "header" | "cookie" | "query") { return Err(AppError::config("set_balancer", format!("pool `{name}` hash_key `{key}` must be ip, uri, header:<name>, cookie:<name> or query:<name>"))); }

            if let Some(sticky) = &pool.options.sticky && (sticky.cookie.is_empty() || sticky.cookie.contains([';', ',', ' ', '=']) || sticky.path.is_empty()) { return Err(AppError::config("set_sticky", format!("pool `{name}` sticky cookie name or path is invalid"))); }

            if name.is_empty() { return Err(AppError::config("add_upstream", "pool name must not be empty")); }

            if pool.backends.is_empty() { return Err(AppError::config("add_upstream", format!("pool `{name}` has no backends"))); }

            backends += pool.backends.len();

            for backend in &pool.backends {

                Self::range(&format!("pool `{name}` weight"), backend.weight as u64, 1, WEIGHT_MAX as u64)?;

                if backend.srv.as_ref().is_some_and(|service| !service.starts_with('_') || service.contains(char::is_whitespace) || backend.address.is_unix()) { return Err(AppError::config("add_upstream", format!("pool `{name}` srv is a service name such as `_http._tcp.api.internal` and replaces the address"))); }

                if backend.tls && backend.server_name.is_empty() && backend.address.name().is_none() {

                    return Err(AppError::config("add_upstream", format!("pool `{name}` backend {} needs server_name for tls", backend.address)));

                }

                if backend.cert.is_some() != backend.key.is_some() { return Err(AppError::config("add_upstream", format!("pool `{name}` backend {} needs both cert and key for a client certificate", backend.address))); }

                if backend.protocol == Protocol::Fastcgi && (backend.tls || pool.backends.iter().any(|other| other.protocol != Protocol::Fastcgi)) { return Err(AppError::config("add_upstream", format!("pool `{name}` mixes fastcgi with other protocols or tls"))); }

                if backend.cert.is_some() && !backend.tls { return Err(AppError::config("add_upstream", format!("pool `{name}` backend {} has a client certificate but tls is off", backend.address))); }

                if backend.tls && backend.address.is_unix() {

                    return Err(AppError::config("add_upstream", format!("pool `{name}` backend {} cannot combine tls with a unix socket", backend.address)));

                }

            }

            Self::range(&format!("pool `{name}` max_fails"), pool.options.max_fails as u64, 1, MAX_FAILS_MAX as u64)?;
            Self::range(&format!("pool `{name}` cooldown_ms"), pool.options.cooldown_ms, COOLDOWN_MS_MIN, COOLDOWN_MS_MAX)?;
            Self::range(&format!("pool `{name}` slow_start_ms"), pool.options.slow_start_ms, 0, SLOW_START_MS_MAX)?;
            Self::range(&format!("pool `{name}` max_ejected"), u64::from(pool.options.max_ejected), 1, 100)?;
            Self::range(&format!("pool `{name}` attempts"), pool.options.attempts as u64, 1, ATTEMPTS_MAX as u64)?;
            Self::range(&format!("pool `{name}` keepalive"), pool.options.keepalive as u64, 0, 4_096)?;
            Self::range(&format!("pool `{name}` slow_ms"), pool.options.slow_ms, 0, TIMEOUT_MS_MAX)?;

            for token in &pool.options.retry_on {

                if !matches!(token.as_str(), "connect" | "error" | "timeout" | "500" | "502" | "503" | "504" | "5xx") {

                    return Err(AppError::config("set_balancer", format!("pool `{name}` retry_on token `{token}` is not one of connect, error, timeout, 500, 502, 503, 504, 5xx")));

                }

            }

            if let Some(health) = &pool.options.health {

                Self::range(&format!("pool `{name}` health.interval_ms"), health.interval_ms, HEALTH_INTERVAL_MS_MIN, HEALTH_INTERVAL_MS_MAX)?;
                Self::range(&format!("pool `{name}` health.timeout_ms"), health.timeout_ms, 10, TIMEOUT_MS_MAX)?;
                Self::range(&format!("pool `{name}` health.status"), health.status as u64, 100, 599)?;

                if let Some(path) = &health.path && !path.starts_with('/') {

                    return Err(AppError::config("set_balancer", format!("pool `{name}` health.path must start with /")));

                }

            }

        }

        if backends > BACKENDS_MAX { return Err(AppError::config("add_upstream", format!("more than {BACKENDS_MAX} backends"))); }

        if let Some(name) = &self.default_pool && !self.pools.contains_key(name) {

            return Err(AppError::config("set_default_upstream", format!("pool `{name}` is not defined")));

        }

        if self.routes.len() > ROUTES_MAX { return Err(AppError::config("add_route", format!("more than {ROUTES_MAX} routes"))); }

        Self::pages("global", &self.error_pages)?;

        for ( position, stream ) in self.streams.iter().enumerate() {

            if stream.name.is_empty() { return Err(AppError::config("add_stream", format!("stream listening on {} needs a name", stream.listen))); }

            if stream.listen.port() == 0 { return Err(AppError::config("add_stream", format!("stream `{}` needs a listen port", stream.name))); }

            if stream.udp && (stream.proxy_protocol.is_some() || !stream.sni.is_empty()) { return Err(AppError::config("add_stream", format!("stream `{}` is udp and cannot use proxy_protocol or sni", stream.name))); }

            if (!stream.udp && stream.listen == self.listen) || self.streams[..position].iter().any(|other| other.listen == stream.listen && other.udp == stream.udp) { return Err(AppError::config("add_stream", format!("stream `{}` listen {} is already in use", stream.name, stream.listen))); }

            if !self.pools.contains_key(&stream.upstream) { return Err(AppError::config("add_stream", format!("stream `{}` points to unknown pool `{}`", stream.name, stream.upstream))); }

            for ( name, pool ) in &stream.sni {

                if name.is_empty() || !self.pools.contains_key(pool) { return Err(AppError::config("add_stream", format!("stream `{}` maps server name `{name}` to unknown pool `{pool}`", stream.name))); }

            }

            if self.pools[&stream.upstream].backends.iter().any(|backend| backend.tls) { return Err(AppError::config("add_stream", format!("stream `{}` cannot use tls backends", stream.name))); }

            Self::range(&format!("stream `{}` idle_ms", stream.name), stream.idle_ms, 1_000, 86_400_000)?;

        }

        if self.listeners.len() > LISTENERS_MAX { return Err(AppError::config("add_listen", format!("more than {LISTENERS_MAX} listeners"))); }

        for ( position, listener ) in self.listeners.iter().enumerate() {

            if listener.address.port() == 0 { return Err(AppError::config("add_listen", format!("listener {} needs a port", listener.address))); }

            if listener.address == self.listen || self.listeners[..position].iter().any(|other| other.address == listener.address) || self.streams.iter().any(|stream| stream.listen == listener.address) { return Err(AppError::config("add_listen", format!("address {} is already in use", listener.address))); }

            if listener.tls && self.tls.is_none() { return Err(AppError::config("add_listen", format!("listener {} needs set_tls", listener.address))); }

            if listener.tls && listener.redirect { return Err(AppError::config("add_listen", format!("listener {} cannot redirect to https and serve tls at once", listener.address))); }

        }

        if self.acl.allow.len() + self.acl.deny.len() > ACL_MAX { return Err(AppError::config("set_acl", format!("more than {ACL_MAX} networks"))); }

        let mut names = HashSet::new();

        for route in &self.routes {

            let label = if route.name.is_empty() { route.path.clone() } else { route.name.clone() };

            if route.name.is_empty() { return Err(AppError::config("add_route", format!("route for `{}` needs a name", route.path))); }

            if !names.insert(route.name.as_str()) { return Err(AppError::config("add_route", format!("route `{label}` is defined twice"))); }

            if !route.path.starts_with('/') { return Err(AppError::config("add_route", format!("route `{label}` path must start with /"))); }

            if route.regex.as_deref().is_some_and(str::is_empty) { return Err(AppError::config("add_route", format!("route `{label}` regex must not be empty"))); }

            if route.regex.is_some() && route.exact { return Err(AppError::config("add_route", format!("route `{label}` cannot be both exact and regex"))); }

            if let Some(pattern) = &route.regex && let Err(error) = regex::Regex::new(pattern) { return Err(AppError::config("add_route", format!("route `{label}` regex is invalid: {error}"))); }

            for rule in route.redirects.iter().flatten().chain(&route.cookie_domain).chain(&route.cookie_path) { if rule.from.is_empty() { return Err(AppError::config("add_route", format!("route `{label}` replacement needs from"))); } }

            Self::pages(&format!("route `{label}`"), &route.error_pages)?;

            for ( position, entry ) in route.try_files.iter().enumerate() {

                let last = position + 1 == route.try_files.len();

                if route.root.is_none() { return Err(AppError::config("add_route", format!("route `{label}` try_files needs root"))); }

                match entry.as_str() {
                    "" => return Err(AppError::config("add_route", format!("route `{label}` try_files entry must not be empty"))),
                    "@upstream" if !last => return Err(AppError::config("add_route", format!("route `{label}` try_files @upstream must be last"))),
                    "@upstream" if !self.pools.contains_key(&route.upstream) => return Err(AppError::config("add_route", format!("route `{label}` try_files @upstream points to unknown pool `{}`", route.upstream))),
                    code if code.starts_with('=') => {

                        if !last { return Err(AppError::config("add_route", format!("route `{label}` try_files status must be last"))); }

                        if !code[1..].parse::<u16>().is_ok_and(|code| (200..=599).contains(&code)) { return Err(AppError::config("add_route", format!("route `{label}` try_files status `{code}` is invalid"))); }

                    }
                    _ => {}
                }

            }

            for rule in &route.rewrite {

                if rule.from.is_empty() || rule.to.is_empty() { return Err(AppError::config("add_route", format!("route `{label}` rewrite needs from and to"))); }

                if let Some(status) = rule.status && !matches!(status, 301 | 302 | 303 | 307 | 308) { return Err(AppError::config("add_route", format!("route `{label}` rewrite status {status} is not a redirect"))); }

            }

            if let Some(index) = &route.index && index.contains(['/', '\\']) { return Err(AppError::config("add_route", format!("route `{label}` index must be a file name"))); }

            if route.root.is_none() && (route.index.is_some() || route.cache_control.is_some()) { return Err(AppError::config("add_route", format!("route `{label}` index and cache_control need root"))); }

            let redirects = !route.rewrite.is_empty() && route.rewrite.iter().all(|rule| rule.status.is_some() || rule.to.starts_with("http://") || rule.to.starts_with("https://"));

            if route.acl.allow.len() + route.acl.deny.len() > ACL_MAX { return Err(AppError::config("add_route", format!("route `{label}` lists more than {ACL_MAX} networks"))); }

            if let Some(respond) = &route.respond {

                Self::range(&format!("route `{label}` respond.status"), u64::from(respond.status), 200, 599)?;

                if respond.body.len() > RESPOND_BYTES_MAX { return Err(AppError::config("add_route", format!("route `{label}` respond body exceeds {RESPOND_BYTES_MAX} bytes"))); }

                if http::header::HeaderValue::from_str(&respond.content_type).is_err() { return Err(AppError::config("add_route", format!("route `{label}` respond content_type is invalid"))); }

            }

            if !route.deny && route.root.is_none() && route.respond.is_none() && !redirects && !self.pools.contains_key(&route.upstream) {

                return Err(AppError::config("add_route", format!("route `{label}` points to unknown pool `{}`", route.upstream)));

            }

            for method in &route.methods {

                if method.is_empty() || !method.bytes().all(|byte| byte.is_ascii_uppercase() || byte == b'-') {

                    return Err(AppError::config("add_route", format!("route `{label}` has invalid method `{method}`")));

                }

            }

            if let Some(host) = &route.host && (host.is_empty() || host.contains(['/', ' '])) {

                return Err(AppError::config("add_route", format!("route `{label}` has invalid host `{host}`")));

            }

            if let Some(timeout) = route.timeout_ms { Self::range(&format!("route `{label}` timeout_ms"), timeout, TIMEOUT_MS_MIN, TIMEOUT_MS_MAX)?; }

            if let Some(mirror) = &route.mirror && !self.pools.contains_key(mirror) { return Err(AppError::config("add_route", format!("route `{label}` mirrors to unknown pool `{mirror}`"))); }

            if let Some(limit) = route.rate_limit_10s { Self::range(&format!("route `{label}` rate_limit_10s"), u64::from(limit), 0, u64::from(RATE_LIMIT_MAX))?; }

            if let Some(rate) = route.rate_per_second { Self::range(&format!("route `{label}` rate_per_second"), u64::from(rate), 0, u64::from(RATE_LIMIT_MAX))?; }

            if let Some(burst) = route.rate_burst { Self::range(&format!("route `{label}` rate_burst"), u64::from(burst), 0, u64::from(RATE_LIMIT_MAX))?; }

            if let Some(limit) = route.concurrency_limit { Self::range(&format!("route `{label}` concurrency_limit"), u64::from(limit), 0, u64::from(CONCURRENCY_MAX))?; }

            Self::rules(&format!("route `{label}`"), &route.rate_rules)?;

            if route.method.as_ref().is_some_and(|method| method.is_empty() || !method.bytes().all(|byte| byte.is_ascii_uppercase())) { return Err(AppError::config("add_route", format!("route `{label}` method is an upper-case token such as GET"))); }

            if route.scheme.as_ref().is_some_and(|scheme| !matches!(scheme.as_str(), "http" | "https")) { return Err(AppError::config("add_route", format!("route `{label}` scheme is http or https"))); }

            if route.charset.as_ref().is_some_and(|charset| charset.is_empty() || !charset.bytes().all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')) { return Err(AppError::config("add_route", format!("route `{label}` charset is a name such as utf-8"))); }

            if let Some(rate) = route.bandwidth { Self::range(&format!("route `{label}` bandwidth"), rate, 0, BANDWIDTH_MAX)?; }

            if let Some(after) = route.bandwidth_after { Self::range(&format!("route `{label}` bandwidth_after"), after, 0, BANDWIDTH_MAX)?; }

            if let Some(auth) = &route.forward_auth {

                if !self.pools.contains_key(&auth.upstream) { return Err(AppError::config("add_route", format!("route `{label}` forward_auth points to unknown pool `{}`", auth.upstream))); }

                if !auth.path.starts_with('/') { return Err(AppError::config("add_route", format!("route `{label}` forward_auth path must start with /"))); }

                Self::range(&format!("route `{label}` forward_auth timeout_ms"), auth.timeout_ms, TIMEOUT_MS_MIN, TIMEOUT_MS_MAX)?;

                for name in &auth.copy_headers { Self::header_name(&format!("route `{label}` forward_auth copy_headers"), name)?; }

            }

            if let Some(auth) = &route.basic_auth {

                if auth.realm.is_empty() { return Err(AppError::config("add_route", format!("route `{label}` basic_auth realm must not be empty"))); }

                if auth.users.is_empty() && auth.users_file.is_none() { return Err(AppError::config("add_route", format!("route `{label}` basic_auth needs users or users_file"))); }

                for ( user, hash ) in &auth.users {

                    if user.is_empty() || user.contains(':') { return Err(AppError::config("add_route", format!("route `{label}` basic_auth user `{user}` is invalid"))); }

                    if !(hash.starts_with("{SHA}") || hash.starts_with("{PLAIN}") || hash.starts_with("$2a$") || hash.starts_with("$2b$") || hash.starts_with("$2y$")) { return Err(AppError::config("add_route", format!("route `{label}` basic_auth user `{user}` needs a bcrypt, {{SHA}} or {{PLAIN}} password hash"))); }

                }

            }

            if let Some(bytes) = route.max_body_bytes { Self::range(&format!("route `{label}` max_body_bytes"), bytes as u64, 1, MAX_BODY_BYTES_MAX as u64)?; }

            Self::headers(&format!("route `{label}` match_headers"), &route.match_headers)?;

            if route.replace.keys().any(String::is_empty) { return Err(AppError::config("add_route", format!("route `{label}` replace cannot search for an empty string"))); }

            if route.root.is_none() && self.pools.get(&route.upstream).is_some_and(|pool| pool.backends.iter().any(|backend| backend.protocol == Protocol::Fastcgi)) { return Err(AppError::config("add_route", format!("route `{label}` sends to a fastcgi pool and needs `root` for the script path"))); }

            if let Some(name) = &route.jwt && !self.jwts.contains_key(name) { return Err(AppError::config("add_route", format!("route `{label}` jwt names `{name}`, which no add_jwt defines"))); }

            for name in &route.cache_bypass { if !self.variables.contains_key(name) { return Err(AppError::config("add_route", format!("route `{label}` cache_bypass names `{name}`, which no add_map, add_geo, add_geoip or add_split defines"))); } }

            for name in route.match_vars.keys() { if !self.variables.contains_key(name) { return Err(AppError::config("add_route", format!("route `{label}` match_vars names `{name}`, which no add_map, add_geo, add_geoip or add_split defines"))); } }
            Self::headers(&format!("route `{label}` request_headers"), &route.request_headers)?;
            Self::headers(&format!("route `{label}` response_headers"), &route.response_headers)?;

        }

        Ok(())

    }

    fn pages ( label: &str, pages: &[ErrorPage] ) -> AppResult<()> {

        for page in pages {

            if page.status.is_empty() { return Err(AppError::config("error_pages", format!("{label} error page needs a status"))); }

            if let Some(status) = page.status.iter().find(|status| !(300..=599).contains(*status)) { return Err(AppError::config("error_pages", format!("{label} error page status {status} is out of range"))); }

            if !(page.page.starts_with('/') || page.page.starts_with("http://") || page.page.starts_with("https://")) { return Err(AppError::config("error_pages", format!("{label} error page `{}` must be a path or an absolute URL", page.page))); }

            if let Some(code) = page.code && !(200..=599).contains(&code) { return Err(AppError::config("error_pages", format!("{label} error page code {code} is out of range"))); }

        }

        Ok(())

    }

    fn rules ( label: &str, rules: &[RateRule] ) -> AppResult<()> {

        if rules.len() > 8 { return Err(AppError::config("rate_rules", format!("{label} takes at most 8 rate_rules"))); }

        for rule in rules {

            Self::range(&format!("{label} rate_rules rate"), u64::from(rule.rate), 1, u64::from(RATE_LIMIT_MAX))?;
            Self::range(&format!("{label} rate_rules burst"), u64::from(rule.burst), 0, u64::from(RATE_LIMIT_MAX))?;

            if !rule.key.is_empty() { HashKey::compile(Some(&rule.key), HashKey::Ip).map_err(|_| AppError::config("rate_rules", format!("{label}: unsupported key `{}`", rule.key)))?; }

        }

        Ok(())

    }

    fn range ( key: &str, value: u64, low: u64, high: u64 ) -> AppResult<()> {

        if value < low || value > high { return Err(AppError::config(key, format!("{value} is outside {low}..={high}"))); }

        Ok(())

    }

    fn header_name ( key: &str, name: &str ) -> AppResult<()> {

        HeaderName::from_bytes(name.as_bytes()).map(|_| ()).map_err(|_| AppError::config(key, format!("`{name}` is not a valid header name")))

    }

    fn headers ( key: &str, headers: &BTreeMap<String, String> ) -> AppResult<()> {

        if headers.len() > HEADERS_MAX { return Err(AppError::config(key, format!("more than {HEADERS_MAX} headers"))); }

        for ( name, value ) in headers {

            let bare = name.strip_prefix(['+', '-', '?']).unwrap_or(name);

            Self::header_name(key, bare)?;

            if name.starts_with('-') && !value.is_empty() { return Err(AppError::config(key, format!("header `{name}` is a removal and takes an empty value"))); }

            if name == "-host" && (key.ends_with("request") || key.ends_with("request_headers")) { return Err(AppError::config(key, "the Host header is set by the proxy and cannot be removed")); }

            if value.len() > HEADER_VALUE_MAX { return Err(AppError::config(key, format!("header `{name}` value exceeds {HEADER_VALUE_MAX} bytes"))); }

            if http::header::HeaderValue::from_str(value).is_err() { return Err(AppError::config(key, format!("header `{name}` has an invalid value"))); }

        }

        Ok(())

    }

}
