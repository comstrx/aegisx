use std::sync::Arc;
use serde_json::json;
use crate::core::{domain::{Actor, hex}, time::now_ms};
use crate::module::{config::Mode, decision::Engine, inference::Envelope, runtime::{Snapshot, RouteState}, storage::Event, verdict::Verdict};
use super::Services;

pub struct AnalysisContext {
    pub snapshot: Arc<Snapshot>,
    pub route: Arc<RouteState>,
    pub actor: Actor,
    pub request_id: String,
    pub sequence: u32,
    pub generation: u64,
    pub capture: bool,
    pub elapsed_ms: u64,
    pub storage: Option<crate::module::storage::Reservation>,
    pub records: Vec<Event>,
    pub slot: Option<crate::module::inference::Reservation>,
}
impl Services {
    pub fn cancellation_notice ( &self, action: &crate::module::verdict::Cancellation ) {
        self.webhooks.emit(json!({"schema_version":1,"event_id":action.action_id,"type":"cancellation_requested",
            "request_id":action.request_id,"route":action.route,"timestamp_ms":now_ms(),"action":action,
            "meaning":"Cooperative request only; delivery does not confirm cancellation"}));
    }

    pub fn analyze ( self: &Arc<Self>, envelope: Envelope, mut work: AnalysisContext ) -> bool {
        let Some(inference) = &self.engine.inference else { return false; };
        if work.slot.is_none() { work.slot=inference.reserve_completion(); }
        if work.slot.is_none() {
            let event=Event { request_id:work.request_id,sequence:work.sequence,stage:"analysis_skipped".into(),
                timestamp_ms:now_ms(),elapsed_ms:work.elapsed_ms,details:json!({"reason":"analysis_queue_full","completed_request_unchanged":true}) };
            if work.capture {
                if work.snapshot.config.control.enabled && work.snapshot.config.telemetry.enabled {self.telemetry.publish(event.clone());}
                work.records.push(event);
                if let Some(reservation)=work.storage {reservation.commit(work.records);}
            }
            return true;
        }
        let services = self.clone();
        let backend_events = if work.capture {envelope.backend_events.clone()} else {Vec::new()};
        inference.background(envelope, work.slot.take(), move |analysis| {
            let current = services.current.load();
            let stale = current.version != work.snapshot.version;
            let signals = analysis.scores.map(|scores| {
                let content = analysis.inputs["request_bytes"].as_u64().unwrap_or(0)>0;
                let journey = analysis.inputs["backend_events"].as_u64().unwrap_or(0)>0;
                (content && scores.content>=work.route.policy.content_threshold,
                 journey && scores.journey>=work.route.policy.journey_threshold)
            }).unwrap_or((false,false));
            let lifecycle = services.engine.model_info.as_ref().is_some_and(|info|info.input_schema.is_some());
            let risky = if lifecycle { signals.0 || signals.1 } else {
                analysis.score.is_some_and(|score|score>=work.route.policy.threshold)
            };
            let mut action = if risky { "observed_risk" } else { "observed" };
            if analysis.expired { action = "expired"; }
            else if analysis.score.is_none() { action = "unavailable"; }
            else if stale { action = "stale_configuration"; }
            else if risky {
                if work.route.policy.mode == Mode::Background && Engine::cache_enabled(&work.snapshot, &work.route)
                    && work.snapshot.config.cache.background_denials
                    && let Some(repository) = &services.engine.verdicts {
                    let key = Engine::key(&work.snapshot, &work.route, &work.actor);
                    let created = now_ms();
                    let verdict = Verdict { key:hex(&key),actor:hex(&work.actor),route:work.route.spec.name.clone(),
                        reason:"background_threshold".into(),source:"model".into(),created_ms:created,
                        expires_ms:created+work.snapshot.config.cache.deny_ttl_ms,request_id:Some(work.request_id.clone()) };
                    action = match repository.submit(key,verdict,Some(work.generation)).and_then(|reply|
                        reply.blocking_recv().map_err(|_| crate::core::error::AppError::invalid("Verdict worker stopped"))?) {
                        Ok(true) => "future_requests_blocked",
                        Ok(false) => "superseded_by_operator",
                        Err(_) => "decision_persistence_failed",
                    };
                }
                if work.route.policy.mode == Mode::Background && work.route.spec.cancellation
                    && let Some(repository)=&services.engine.verdicts && repository.generation()==work.generation {
                    let action=crate::module::verdict::Cancellation::new(work.request_id.clone(),work.route.spec.name.clone(),
                        "background_risk".into(),work.route.spec.cancellation_ttl_ms);
                    if let Ok(reply)=repository.submit_cancellation(action,Some(work.generation)) && let Ok(Ok(action))=reply.blocking_recv() {
                        services.cancellation_notice(&action);
                    }
                }
                services.hook("risk_detected",&work.request_id,&work.route.spec.name,&work.actor,"background_threshold");
            }
            if work.capture {
                let event=Event { request_id:work.request_id, sequence:work.sequence, stage:"analyzed".into(),
                    timestamp_ms:now_ms(), elapsed_ms:work.elapsed_ms+analysis.queue_ms+analysis.inference_us/1000,
                    details:json!({"decision":"completed_request_unchanged","action":action,
                        "model_inputs":analysis.inputs,"component_scores":analysis.scores,
                        "thresholds":{"content":work.route.policy.content_threshold,"journey":work.route.policy.journey_threshold,
                            "legacy":work.route.policy.threshold},
                        "triggered_signals":{"content":signals.0,"journey":signals.1},
                        "features":analysis.features.to_vec(),"feature_version":crate::module::inference::FEATURE_VERSION,"risk_score":analysis.score,
                        "queue_ms":analysis.queue_ms,"inference_us":analysis.inference_us,"journal_us":analysis.journal_us,
                        "request_elapsed_ms":work.elapsed_ms,"config_version":work.snapshot.version,
                        "route":work.route.spec.name,"actor":hex(&work.actor),
                        "model_sha256":services.engine.model_info.as_ref().map(|info|&info.artifact_sha256),
                        "lifecycle_event_count":work.records.len(),"backend_events":backend_events,
                        "capture":"bounded_metadata_and_content_statistics; raw payload discarded"}) };
                if work.snapshot.config.control.enabled && work.snapshot.config.telemetry.enabled { services.telemetry.publish(event.clone()); }
                work.records.push(event);
                if let Some(reservation)=work.storage { reservation.commit(work.records); }
            }
        })
    }
}
