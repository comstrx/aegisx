use std::collections::BTreeMap;
use std::sync::Arc;

use uuid::Uuid;

use crate::core::error::{AppError, AppResult};
use crate::module::config::{Config, Route};
use crate::module::upstream::Pool;
use super::{Policy, RouteState, Snapshot};

impl Snapshot {

    pub fn build ( config: Config, previous: Option<&Self> ) -> AppResult<Self> {

        config.validate()?;
        let mut pools = BTreeMap::new();
        for (name, settings) in &config.pools {
            let reused = previous.and_then(|old| old.pools.get(name)).filter(|pool| pool.config == *settings);
            let pool = match reused {
                Some(pool) => pool.clone(),
                None => Arc::new(Pool::new(name.clone(), settings.clone(), config.control.enabled)?),
            };
            pools.insert(name.clone(), pool);
        }
        let mut routes: Vec<_> = config.routes.iter().map(|route| Arc::new(RouteState::build(route.clone(), &config, &pools))).collect();
        routes.sort_by(|left, right| {
            let rank = |route: &RouteState| match &route.spec.host {
                None => 0, Some(host) if host.starts_with("*.") => 1, Some(_) => 2,
            };
            rank(right).cmp(&rank(left))
                .then(right.spec.host.as_ref().map(String::len).cmp(&left.spec.host.as_ref().map(String::len)))
                .then(right.spec.path.len().cmp(&left.spec.path.len()))
                .then(right.spec.exact.cmp(&left.spec.exact))
                .then(left.spec.methods.is_empty().cmp(&right.spec.methods.is_empty()))
                .then(left.spec.methods.len().cmp(&right.spec.methods.len()))
                .then(right.spec.match_headers.len().cmp(&left.spec.match_headers.len()))
                .then(left.spec.name.cmp(&right.spec.name))
        });
        let fallback = config.default_pool.as_ref().map(|name| Arc::new(RouteState::build(Route {
            name: "default".into(), upstream: name.clone(), ..Route::default()
        }, &config, &pools)));

        let index = super::index::RouteIndex::build(&routes);
        Ok(Self { identity_headers:crate::module::identity::Headers::new(&config.identity), index, version: Uuid::new_v4().to_string(), config, pools, routes, fallback })

    }

    pub fn compatible ( &self, next: &Config, model_available: bool ) -> AppResult<()> {

        let old = &self.config;
        if old.queue != next.queue || old.persistence.synchronous != next.persistence.synchronous || old.context != next.context || old.control != next.control || old.webhooks != next.webhooks
            || old.cache.resources() != next.cache.resources() || old.cache.lookup_timeout_ms != next.cache.lookup_timeout_ms || old.cache.write_timeout_ms != next.cache.write_timeout_ms || old.cache.decision_ttl_ms != next.cache.decision_ttl_ms || old.telemetry.recent_capacity != next.telemetry.recent_capacity
            || old.listen != next.listen || old.tls != next.tls || old.store != next.store || old.runtime != next.runtime
            || old.limits.max_sources != next.limits.max_sources || old.limits.queue_capacity != next.limits.queue_capacity
            || old.limits.retention_events != next.limits.retention_events
            || old.model.max_queue_age_ms != next.model.max_queue_age_ms || old.model.directory != next.model.directory || old.model.queue_capacity != next.model.queue_capacity
            || (!model_available && next.needs_model())
        { return Err(AppError::invalid("Listener, TLS, storage, worker resources or model artifact changes require restart")); }

        Ok(())

    }

}

impl RouteState {
    fn headers ( global: &BTreeMap<String,String>, local: &BTreeMap<String,String> ) -> Vec<(http::HeaderName,http::HeaderValue)> {
        let values:BTreeMap<_,_>=global.iter().chain(local).map(|(name,value)|(name.to_ascii_lowercase(),value)).collect();
        values.into_iter().map(|(name,value)| (
            http::HeaderName::from_bytes(name.as_bytes()).expect("validated policy header"),
            http::HeaderValue::from_str(value).expect("validated policy value")
        )).collect()
    }


    fn build ( spec: Route, config: &Config, pools: &BTreeMap<String, Arc<Pool>> ) -> Self {

        let mut limits = config.limits.clone();
        if let Some(value) = spec.timeout_ms { limits.timeout_ms = value; }
        if let Some(value) = spec.max_body_bytes { limits.max_body_bytes = value; }
        if let Some(value) = spec.rate_limit_10s { limits.rate_limit_10s = value; }
        let request_headers = Self::headers(&config.request_headers,&spec.request_headers);
        let response_headers = Self::headers(&config.response_headers,&spec.response_headers);
        let policy = Policy {
            limits, mode: spec.model.unwrap_or(config.model.mode),
            threshold: spec.threshold.unwrap_or(config.model.threshold),
            content_threshold: spec.threshold.or(config.model.content_threshold).unwrap_or(config.model.threshold),
            journey_threshold: spec.threshold.or(config.model.journey_threshold).unwrap_or(config.model.threshold),
            capture: (config.store.is_some() || (config.control.enabled && config.telemetry.enabled)) && spec.capture.unwrap_or(true),
            preserve_host: spec.preserve_host.unwrap_or(config.preserve_host),
            request_headers, response_headers,
        };

        // Preserve v0.4 persisted keys; new queue-overload settings do not change actor policy identity.
        let mut model_identity=format!("ModelConfig {{ mode: {:?}, scan_bytes: {}, max_queue_age_ms: {}, directory: {:?}, threshold: {:?}, queue_capacity: {}, allow_unvalidated: {} }}",
            config.model.mode,config.model.scan_bytes,config.model.max_queue_age_ms,config.model.directory,
            config.model.threshold,config.model.queue_capacity,config.model.allow_unvalidated);
        if config.model.response_scan_bytes>0 || !config.model.journey {
            model_identity.push_str(&format!(":response={}:journey={}",config.model.response_scan_bytes,config.model.journey));
        }
        if config.model.content_threshold.is_some() || config.model.journey_threshold.is_some() {
            model_identity.push_str(&format!(":content_threshold={:?}:journey_threshold={:?}",
                config.model.content_threshold,config.model.journey_threshold));
        }
        let decision_namespace = crate::module::cache::Caches::key(&[format!("v2:{:?}:{:?}:{}:{:?}",
            spec, config.identity, model_identity, config.cache).as_bytes()]);
        Self { pool:pools.get(&spec.upstream).cloned(), decision_namespace, name: Arc::from(spec.name.as_str()), spec, policy }

    }

}
