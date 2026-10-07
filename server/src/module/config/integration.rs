use std::collections::HashSet;
use crate::core::error::{AppError, AppResult};
use super::Config;

impl Config {

    pub(super) fn validate_integrations ( &self ) -> AppResult<()> {

        if !(1..=64).contains(&self.context.shards) || !(10000..=86400000).contains(&self.context.idle_ttl_ms) {
            return Err(AppError::invalid("Context requires 1–64 shards and idle TTL 10 seconds–24 hours"));
        }
        let cache = &self.cache;
        if !(1..=100000).contains(&cache.max_entries) || !(1024..=268435456).contains(&cache.max_bytes)
            || !(1..=1048576).contains(&cache.max_object_bytes) || cache.max_object_bytes as u64 > cache.max_bytes
            || [cache.deny_ttl_ms, cache.score_ttl_ms, cache.response_ttl_ms].iter().any(|value| !(1..=600000).contains(value))
        { return Err(AppError::invalid("Cache budgets outside supported bounds")); }
        let identity = &self.identity;
        let names = [&identity.request_id_header, &identity.forwarded_for_header, &identity.forwarded_proto_header]
            .into_iter().chain(identity.actor_header.iter()).chain(identity.backend_block_header.iter());
        let mut seen = HashSet::new();
        for name in names {
            if !header_name(name) || !seen.insert(name.to_ascii_lowercase()) {
                return Err(AppError::invalid("Integration header names must be valid and distinct"));
            }
            let key = name.to_ascii_lowercase();
            if self.request_headers.keys().chain(self.response_headers.keys())
                .chain(self.routes.iter().flat_map(|route| route.request_headers.keys().chain(route.response_headers.keys())))
                .any(|value| value.eq_ignore_ascii_case(&key))
            { return Err(AppError::invalid("Custom headers cannot overwrite integration headers")); }
        }
        if identity.trusted_peers.len() > 32 { return Err(AppError::invalid("At most 32 trusted peer networks")); }
        if (identity.actor_header.is_some() || identity.preserve_trusted_id) && identity.trusted_peers.is_empty() {
            return Err(AppError::invalid("Trusted identity headers require explicit trusted_peers"));
        }
        let telemetry = &self.telemetry;
        if !(1..=4096).contains(&telemetry.recent_capacity) || !(1..=10000).contains(&telemetry.sample_every)
            || !matches!(telemetry.capture.as_str(), "full" | "summary")
        { return Err(AppError::invalid("Invalid telemetry settings")); }
        let control = &self.control;
        if control.backend_token_env.as_ref().is_some_and(|name| !environment_name(name) || name == &control.token_env) {
            return Err(AppError::invalid("Backend credentials require a distinct valid environment name"));
        }
        if control.enabled && (!control.listen.ip().is_loopback() || control.listen.port() == 0
            || (control.listen.port() == self.listen.port() && (self.listen.ip().is_unspecified() || control.listen.ip() == self.listen.ip())) || !environment_name(&control.token_env)
            || !control.api_prefix.starts_with('/') || control.api_prefix.ends_with('/')
            || !control.api_prefix.bytes().all(|value| value.is_ascii_alphanumeric() || b"/_-".contains(&value)))
        { return Err(AppError::invalid("Control API requires a distinct loopback listener, safe prefix and token_env")); }
        let hooks = &self.webhooks;
        if !(1..=4096).contains(&hooks.queue_capacity) || !(100..=10000).contains(&hooks.timeout_ms)
            || !(1..=5).contains(&hooks.attempts) || hooks.endpoints.len() > 8
        { return Err(AppError::invalid("Invalid webhook limits")); }
        let mut names = HashSet::new();
        for endpoint in &hooks.endpoints {
            let url = reqwest::Url::parse(&endpoint.url).map_err(|_| AppError::invalid("Invalid webhook URL"))?;
            let local = url.host_str().is_some_and(|host| host.parse::<std::net::IpAddr>().is_ok_and(|ip| ip.is_loopback()));
            if (url.scheme() != "https" && !(url.scheme() == "http" && local))
                || !url.username().is_empty() || url.password().is_some() || url.fragment().is_some()
                || !environment_name(&endpoint.secret_env) || !names.insert(&endpoint.name)
                || endpoint.name.is_empty() || endpoint.name.len() > 64
                || endpoint.events.iter().any(|event| !matches!(event.as_str(), "blocked" | "risk_detected" | "backend_signal" | "cancellation_requested"))
            { return Err(AppError::invalid("Webhooks need HTTPS (or loopback HTTP), unique names, secret_env and known events")); }
        }

        Ok(())

    }

}

pub(super) fn header_name ( name: &str ) -> bool {

    let key = name.to_ascii_lowercase();
    !name.is_empty() && name.len() <= 64 && name.bytes().all(|value| value.is_ascii_alphanumeric() || value == b'-')
        && !matches!(key.as_str(), "host" | "content-length" | "transfer-encoding" | "connection" | "upgrade"
            | "authorization" | "cookie" | "set-cookie" | "te" | "trailer" | "proxy-authorization")

}

pub(super) fn environment_name ( name: &str ) -> bool {

    !name.is_empty() && name.len() <= 128 && name.bytes().all(|value| value.is_ascii_uppercase() || value.is_ascii_digit() || value == b'_')

}
