use std::sync::Arc;
use std::time::Instant;
use arc_swap::ArcSwap;
use serde_json::{Value, json};

use crate::core::{domain::{Actor, hex}, error::AppResult, time::now_ms};
use crate::module::{
    cache::Caches, config::Config, decision::Engine, identity::Identity,
    inference::{Inference, InferenceGuard, Model}, runtime::Snapshot,
    storage::{Store, StoreGuard}, telemetry::Telemetry, webhook::{Webhooks, WebhookGuard},
};

pub struct Services {
    pub current: Arc<ArcSwap<Snapshot>>,
    pub engine: Engine,
    pub caches: Arc<Caches>,
    pub identity: Identity,
    pub telemetry: Arc<Telemetry>,
    pub webhooks: Webhooks,
    pub store: Store,
    pub capacity: Arc<crate::core::capacity::Capacity>,
    pub queue: crate::module::queue::Queue,
    pub started: Instant,
    pub resources: crate::module::telemetry::Resources,
    pub journeys: crate::module::lifecycle::Journeys,
}

pub struct ServiceGuards {
    inference: Option<InferenceGuard>,
    verdict: Option<crate::module::verdict::VerdictGuard>,
    storage: Option<StoreGuard>,
    webhook: Option<WebhookGuard>,
}

impl Services {

    pub fn start ( config: Config ) -> AppResult<(Arc<Self>, ServiceGuards)> {

        let started = Instant::now();
        let database=crate::core::WriteGate::default();
        let model = config.needs_model().then(|| Model::load_at(config.model.directory.as_deref())).transpose()?;
        let info = model.as_ref().map(|model| model.info.clone());
        Model::validate_policy(&config, info.as_ref())?;
        let current = Arc::new(ArcSwap::from_pointee(Snapshot::build(config.clone(), None)?));
        let caches = Arc::new(Caches::new(&config.cache));
        let (webhooks, webhook) = Webhooks::start(&config.webhooks)?;
        let (store, storage) = match &config.store {
            Some(path) => { let (store, guard) = Store::open(path, config.limits.queue_capacity, config.limits.retention_events, &config.persistence.synchronous,database.clone())?; (store, Some(guard)) }
            None => (Store::disabled(), None),
        };
        let (verdicts, verdict, identity_key) = match &config.store {
            Some(path) => { let (repository, guard, key) = crate::module::verdict::Verdicts::open(path, &config.cache,database.clone())?; (Some(repository), Some(guard), Some(key)) }
            None => (None,None,None),
        };
        let (inference, inference_guard) = match model {
            Some(model) => { let (worker, guard) = Inference::start(&config.model, model, caches.clone(),config.store.as_deref().map(|path|(path,config.persistence.synchronous.as_str(),database.clone())))?; (Some(worker), Some(guard)) }
            None => (None, None),
        };
        let service = Self {
            engine: Engine::new(&config, verdicts, inference, info, started)?,
            journeys: crate::module::lifecycle::Journeys::new(config.runtime.max_in_flight + config.queue.capacity),
            current, caches, identity: Identity::new(identity_key.as_deref())?, store, webhooks,
            telemetry: Arc::new(Telemetry::new(config.telemetry.recent_capacity)),
            queue: crate::module::queue::Queue::new(&config.queue),
            capacity: Arc::new(crate::core::capacity::Capacity::new(config.runtime.max_in_flight)), resources:Default::default(), started,
        };

        Ok((Arc::new(service), ServiceGuards { verdict, inference: inference_guard, storage, webhook }))

    }

    pub fn hook ( &self, kind: &str, request_id: &str, route: &str, actor: &Actor, reason: &str ) {

        if !self.current.load().config.webhooks.enabled { return; }
        self.webhooks.emit(json!({
            "schema_version": 1, "event_id": uuid::Uuid::new_v4().to_string(),
            "type": kind, "request_id": request_id, "timestamp_ms": now_ms(),
            "route": route, "actor": hex(actor), "reason": reason,
        }));

    }

    pub fn state ( &self ) -> Value {

        let snapshot = self.current.load();
        let now = self.started.elapsed().as_millis() as u64;
        let configuration=json!({
                "queue_capacity":snapshot.config.queue.capacity,"queue_timeout_ms":snapshot.config.queue.timeout_ms,
                "threads":snapshot.config.runtime.threads,"work_stealing":snapshot.config.runtime.work_stealing,
                "accept_tasks":snapshot.config.runtime.accept_tasks,"upstream_keepalive_capacity":snapshot.config.runtime.upstream_keepalive_capacity,
                "write_buffer_bytes":snapshot.config.runtime.write_buffer_bytes,"keepalive_seconds":snapshot.config.runtime.keepalive_seconds,
                "scan_bytes":snapshot.config.model.scan_bytes,"response_scan_bytes":snapshot.config.model.response_scan_bytes,"journey":snapshot.config.model.journey,"on_overload":snapshot.config.model.on_overload,
                "feature_count":crate::module::inference::FEATURE_COUNT,"feature_version":crate::module::inference::FEATURE_VERSION,
                "route_count":snapshot.routes.len(),"routes":snapshot.routes.iter().map(|route|json!({
                    "name":route.spec.name,"path":route.spec.path,"host":route.spec.host,"methods":route.spec.methods,
                    "upstream":route.spec.upstream,"capture":route.policy.capture,"model":format!("{:?}",route.policy.mode).to_ascii_lowercase(),
                    "rate_limit_10s":route.policy.limits.rate_limit_10s,"deny":route.spec.deny,"exact":route.spec.exact
                })).collect::<Vec<_>>()
            });
        json!({
            "schema_version": 1, "version": env!("CARGO_PKG_VERSION"),
            "config_version": snapshot.version, "uptime_ms": now,
            "queue": self.queue.stats(), "telemetry": self.telemetry.snapshot(), "cache": self.caches.stats(),
            "webhooks": self.webhooks.stats(), "dropped_events": self.store.dropped(), "storage":self.store.stats(),
            "model": self.engine.model_info,
            "analysis": self.engine.inference.as_ref().map(|worker| worker.stats()),
            "decisions": self.engine.verdicts.as_ref().map(|store| store.stats()),
            "journeys": self.journeys.snapshot(), "resources":self.resources.snapshot(),
            "context": {"sources":self.engine.history.global.sources(), "route_sources":self.engine.history.routes.sources(),
                "shards":snapshot.config.context.shards, "idle_ttl_ms":snapshot.config.context.idle_ttl_ms},
            "configuration": configuration,
            "policies": { "persistence_admission":snapshot.config.persistence.admission,"persistence_synchronous":snapshot.config.persistence.synchronous, "model": format!("{:?}", snapshot.config.model.mode).to_ascii_lowercase(),
                "rate_limit_10s": snapshot.config.limits.rate_limit_10s,
                "max_in_flight": snapshot.config.runtime.max_in_flight,
                "decision_cache": snapshot.config.cache.decisions, "deny_ttl_ms":snapshot.config.cache.deny_ttl_ms,
                "cancellable_routes":snapshot.routes.iter().filter(|route|route.spec.cancellation).map(|route|&route.spec.name).collect::<Vec<_>>(), "response_cache": snapshot.config.cache.responses },
            "upstreams": snapshot.pools.values().map(|pool| pool.stats(now)).collect::<Vec<_>>(),
        })

    }

}

impl ServiceGuards {
    pub fn finish ( self ) -> AppResult<()> {
        let inference = self.inference.map(|guard| guard.finish()).transpose();
        let webhook = self.webhook.map(|guard| guard.finish()).transpose();
        let verdict = self.verdict.map(|guard| guard.finish()).transpose();
        let storage = self.storage.map(|guard| guard.finish()).transpose();
        inference?; webhook?; verdict?; storage?;
        Ok(())
    }
}
