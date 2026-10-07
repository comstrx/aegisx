use std::sync::Arc;
use std::time::Instant;

use crate::core::{domain::{Actor, Facts}, error::AppResult};
use crate::module::{cache::Caches, config::{Config, Mode}, inference::{Features, Inference, ModelInfo},
    memory::Memory, runtime::{RouteState, Snapshot}};
use super::{Engine, History, HistoryGuard, Inspection};

impl Engine {

    pub fn new ( config: &Config, verdicts: Option<crate::module::verdict::Verdicts>, inference: Option<Inference>, model_info: Option<ModelInfo>, started: Instant ) -> AppResult<Self> {

        Ok(Self {
            history: Arc::new(History { global: Memory::configured(config.limits.max_sources, config.context.shards, config.context.idle_ttl_ms),
                routes: Memory::configured(config.limits.max_sources, config.context.shards, config.context.idle_ttl_ms), started }),
            verdicts, inference, model_info, features: Features::load()?,
        })

    }

    pub fn key ( _snapshot: &Snapshot, route: &RouteState, actor: &Actor ) -> [u8; 32] {

        Caches::key(&[&route.decision_namespace, actor])

    }

    pub fn cache_enabled ( snapshot: &Snapshot, route: &RouteState ) -> bool {

        snapshot.config.cache.decisions && route.spec.decision_cache.unwrap_or(true)

    }

    pub async fn restriction ( &self, actor: Actor, snapshot: &Snapshot, route: &RouteState ) -> Inspection {
        let mut result = Inspection { model_state: "off", ..Inspection::default() };
        let key = Self::cache_enabled(snapshot, route).then(|| Self::key(snapshot, route, &actor));
        if let Some(key) = key && let Some(repository) = &self.verdicts {
          let denial = match repository.get(key).await {
            Ok(value) => value,
            Err(_) => {
                if snapshot.config.cache.on_lookup_failure == "deny" {
                    result.status=Some(503); result.reason="decision_store_unavailable".into(); return result;
                }
                None
            }
          };
          if let Some(denial) = denial {
            result.status = Some(403);
            result.reason = denial.reason.clone();
            result.denial_cached = true;
            result.model_rejected = denial.source == "model";
            result.cached = true;
            return result;
          }
        }
        result
    }

    pub async fn inspect ( &self, facts: &Facts, actor: Actor, snapshot: &Snapshot, route: &RouteState, capture: bool ) -> Inspection {

        let passive = route.policy.mode == Mode::Off && snapshot.config.limits.rate_limit_10s == 0
            && route.spec.rate_limit_10s.unwrap_or(0) == 0 && !capture;
        // The independent forwarding mode needs neither a repository future nor behavioral history.
        if passive && !Self::cache_enabled(snapshot, route) {
            return Inspection { model_state: "off", ..Inspection::default() };
        }
        let mut result = self.restriction(actor, snapshot, route).await;
        if result.status.is_some() || passive { return result; }
        let mut guard = HistoryGuard::new(self.history.clone(), actor);
        let now = self.history.now();
        let limit = snapshot.config.limits.rate_limit_10s;
        let previous = if route.policy.mode != Mode::Off || limit > 0 || capture {
            match self.history.global.begin(actor, now) {
                Some(previous) => {
                    guard.global = true;
                    if limit > 0 && previous.requests >= limit {
                        result.status = Some(429); result.reason = "peer_rate_limit".into();
                    }
                    Some(previous)
                }
                None => { result.status = Some(503); result.reason = "source_capacity".into(); None }
            }
        } else { None };
        if result.status.is_none() && let Some(limit) = route.spec.rate_limit_10s.filter(|limit| *limit > 0) {
            match self.history.routes.begin((route.name.clone(), actor), now) {
                Some(previous) => {
                    guard.route = Some(route.name.clone());
                    if previous.requests >= limit { result.status = Some(429); result.reason = "route_rate_limit".into(); }
                }
                None => { result.status = Some(503); result.reason = "route_source_capacity".into(); }
            }
        }
        result.guard = Some(guard);
        if result.status.is_some() { return result; }
        result.features = previous.map(|previous| self.features.extract(facts, previous));
        let mode = route.policy.mode;
        result.model_state = if mode == Mode::Off { "off" } else { "deferred" };

        result

    }

}
