use std::collections::{BTreeMap, HashSet};
use std::net::SocketAddr;

use crate::core::error::{AppError, AppResult};
use super::{Config, Limits};

impl Config {

    pub fn validate ( &self ) -> AppResult<()> {

        if self.listen.port() == 0 { return Err(AppError::invalid("Listen port must be nonzero")); }
        self.validate_integrations()?;
        if !matches!(self.persistence.admission.as_str(), "required"|"best_effort")
            || !matches!(self.persistence.synchronous.as_str(), "full"|"normal") {
            return Err(AppError::invalid("Persistence requires admission=required|best_effort and synchronous=full|normal"));
        }
        if self.queue.capacity > 8192 || !(1..=30000).contains(&self.queue.timeout_ms) { return Err(AppError::invalid("Queue capacity must be 0..8192 and timeout_ms 1..30000")); }
        check_limits(&self.limits)?;
        let runtime = &self.runtime;
        if !(1..=64).contains(&runtime.accept_tasks) || !(1..=65536).contains(&runtime.upstream_keepalive_capacity)
            || !(1..=64).contains(&runtime.threads) || !(1..=100000).contains(&runtime.max_in_flight)
            || runtime.write_buffer_bytes > 65536 || runtime.keepalive_seconds > 600 || !(1..=600).contains(&runtime.pool_idle_seconds)
        { return Err(AppError::invalid("Runtime limits outside supported bounds")); }
        let model = &self.model;
        if model.mode == super::Mode::Enforce || self.routes.iter().any(|route| route.model == Some(super::Mode::Enforce)) {
            return Err(AppError::invalid("Inline enforcement was removed: use background with background_denials"));
        }
        if self.cache.decisions && self.store.is_none() { return Err(AppError::invalid("Decisions require set_store: SQLite is authoritative")); }
        if !(100..=10000).contains(&self.cache.write_timeout_ms) || !(1..=100).contains(&self.cache.lookup_timeout_ms) || !(1..=60000).contains(&self.cache.decision_ttl_ms)
            || !matches!(self.cache.on_lookup_failure.as_str(), "allow" | "deny")
            || model.response_scan_bytes > crate::module::inference::TEXT_BYTES || !(0..=65536).contains(&model.scan_bytes) || !(1..=60000).contains(&model.max_queue_age_ms) {
            return Err(AppError::invalid("Invalid decision lookup or background analysis budget"));
        }
        if !matches!(model.on_overload.as_str(),"reject"|"skip") || !model.threshold.is_finite() || !(0.0..=1.0).contains(&model.threshold)
            || [model.content_threshold,model.journey_threshold].into_iter().flatten().any(|value| !value.is_finite() || !(0.0..=1.0).contains(&value))
            || !(1..=4096).contains(&model.queue_capacity)
        { return Err(AppError::invalid("Invalid model policy or resource budget")); }
        if self.pools.len() > 32 || self.pools.values().map(|pool| pool.backends.len()).sum::<usize>() > 64 {
            return Err(AppError::invalid("At most 32 pools and 64 upstreams are supported"));
        }
        for (name, pool) in &self.pools {
            if !valid_name(name) || pool.backends.is_empty() { return Err(AppError::invalid("Pools need a valid name and at least one backend")); }
            let mut addresses = HashSet::new();
            for backend in &pool.backends {
                check_address(self.listen, backend.address)?;
                if !addresses.insert(backend.address) || !(1..=1000).contains(&backend.weight) || backend.max_in_flight > 100000 {
                    return Err(AppError::invalid("Invalid or duplicate backend, weight or concurrency"));
                }
                if backend.tls && (backend.server_name.is_empty() || backend.server_name.len() > 253
                    || !backend.server_name.bytes().all(|value| value.is_ascii_alphanumeric() || b".-".contains(&value)))
                { return Err(AppError::invalid("TLS upstreams require a valid server_name for certificate verification")); }
                if !backend.tls && backend.ca_file.is_some() { return Err(AppError::invalid("ca_file requires upstream TLS")); }
            }
            let options = &pool.options;
            if !(200..=599).contains(&options.health_status) || options.health_path.as_ref().is_some_and(|path| {
                !path.starts_with('/') || path.starts_with("//") || path.len() > 2048 || path.contains('#')
                    || path.bytes().any(|value| value.is_ascii_control() || value == b' ')
            }) { return Err(AppError::invalid("Invalid HTTP health path or expected status")); }
            if !(1..=10).contains(&options.max_fails) || !(100..=600000).contains(&options.cooldown_ms)
                || !(50..=1000).contains(&options.health_timeout_ms)
                || (options.health_interval_ms != 0 && !(100..=60000).contains(&options.health_interval_ms))
                || !(1..=3).contains(&options.connect_attempts)
            { return Err(AppError::invalid("Invalid balancing or health-check budget")); }
        }
        if self.default_pool.as_ref().is_some_and(|name| !self.pools.contains_key(name)) {
            return Err(AppError::invalid("Default upstream pool does not exist"));
        }
        if self.routes.len() > 1024 { return Err(AppError::invalid("At most 1024 routes are supported")); }
        let mut names = HashSet::new();
        let mut matches = HashSet::new();
        for route in &self.routes {
            if !(1000..=600000).contains(&route.cancellation_ttl_ms)
                || (route.cancellation && (self.store.is_none() || !self.control.enabled || self.control.backend_token_env.is_none())) {
                return Err(AppError::invalid("Cooperative cancellation requires storage, backend integration and a 1s–10m deadline"));
            }
            if !valid_name(&route.name) || !names.insert(&route.name) || route.name == "default" {
                return Err(AppError::invalid("Routes require unique names other than default"));
            }
            if !route.deny && !self.pools.contains_key(&route.upstream) { return Err(AppError::invalid("Route references an unknown pool")); }
            if !route.path.starts_with('/') || route.path.contains(['?', '#', '%', '\\']) || route.path.contains("//")
                || route.path.split('/').any(|part| matches!(part, "." | "..")) || route.path.len() > 2048
            { return Err(AppError::invalid("Route path must be an unambiguous absolute path")); }
            if route.strip_prefix && route.exact { return Err(AppError::invalid("strip_prefix requires a prefix route")); }
            if route.host.as_ref().is_some_and(|host| host.is_empty() || host.len() > 253
                || !host.strip_prefix("*.").unwrap_or(host).bytes().all(|value| value.is_ascii_alphanumeric() || b".-".contains(&value))
                || host.strip_prefix("*.").is_some_and(|suffix| suffix.is_empty() || suffix.starts_with('.')))
            { return Err(AppError::invalid("Route host must be a DNS host or leading wildcard suffix without a port")); }
            if route.methods.iter().any(|method| !matches!(method.as_str(), "GET" | "HEAD" | "POST" | "PUT" | "PATCH" | "DELETE" | "OPTIONS" | "TRACE"))
            { return Err(AppError::invalid("Unsupported route method")); }
            let mut methods = route.methods.clone();
            methods.sort();
            if !matches.insert((route.host.clone(), route.path.clone(), route.exact, methods, route.match_headers.clone())) {
                return Err(AppError::invalid("Duplicate route match"));
            }
            let mut limits = self.limits.clone();
            if let Some(value) = route.timeout_ms { limits.timeout_ms = value; }
            if let Some(value) = route.max_body_bytes { limits.max_body_bytes = value; }
            check_limits(&limits)?;
            if route.threshold.is_some_and(|value| !value.is_finite() || !(0.0..=1.0).contains(&value)) {
                return Err(AppError::invalid("Route threshold must be in [0, 1]"));
            }
            check_headers(&route.match_headers)?;
            check_headers(&route.request_headers)?;
            check_headers(&route.response_headers)?;
        }
        check_headers(&self.request_headers)?;
        check_headers(&self.response_headers)?;

        Ok(())

    }

}

fn valid_name ( name: &str ) -> bool {

    !name.is_empty() && name.len() <= 64 && name.bytes().all(|value| value.is_ascii_alphanumeric() || b"_-.".contains(&value))

}

fn check_address ( listen: SocketAddr, upstream: SocketAddr ) -> AppResult<()> {

    if upstream.port() == 0 || upstream.ip().is_unspecified() || upstream.ip().is_multicast()
        || listen == upstream || (listen.ip().is_unspecified() && listen.port() == upstream.port())
    { return Err(AppError::invalid("Invalid upstream address or proxy loop")); }

    Ok(())

}

fn check_limits ( limits: &Limits ) -> AppResult<()> {

    if !(100..=120000).contains(&limits.timeout_ms) || !(1..=16777216).contains(&limits.max_body_bytes)
        || !(1..=100000).contains(&limits.max_sources) || !(16..=65536).contains(&limits.queue_capacity)
        || !(100..=1000000).contains(&limits.retention_events)
    { return Err(AppError::invalid("Limits outside supported bounds; see server/README.md")); }

    Ok(())

}

fn check_headers ( headers: &BTreeMap<String, String> ) -> AppResult<()> {

    if headers.len() > 32 { return Err(AppError::invalid("At most 32 custom headers per scope")); }
    let mut probe = pingora::http::RequestHeader::build("GET", b"/", None).map_err(|_| AppError::invalid("Cannot validate headers"))?;
    let mut names=HashSet::new();
    for (name, value) in headers {
        let key = name.to_ascii_lowercase();
        if !names.insert(key.clone()) {return Err(AppError::invalid("Duplicate custom header names are case-insensitive"));}
        if matches!(key.as_str(), "host" | "content-length" | "transfer-encoding" | "connection" | "upgrade"
            | "te" | "trailer" | "keep-alive" | "proxy-connection" | "x-request-id" | "forwarded" | "x-real-ip")
            || key.starts_with("x-forwarded-") || value.len() > 4096
        { return Err(AppError::invalid("Transport, identity and framing headers cannot be overridden")); }
        probe.insert_header(name.clone(), value.clone()).map_err(|_| AppError::invalid("Invalid custom header name or value"))?;
    }

    Ok(())

}
