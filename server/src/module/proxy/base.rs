use std::sync::Arc;
use std::time::Instant;
use pingora::{http::ResponseHeader, proxy::Session};
use serde_json::{Value, json};
use uuid::Uuid;

use crate::core::{domain::hex, time::now_ms};
use crate::module::{decision::Inspection, runtime::Snapshot, services::Services, storage::Event};
use super::arch::{Context, Proxy};

impl Proxy {

    pub fn new ( services: Arc<Services> ) -> Self { Self { services } }

    pub(super) fn record ( &self, context: &mut Context, stage: &str, details: impl FnOnce(&Context) -> Value ) {

        if !context.capture && context.journey.is_none() { return; }
        context.sequence += 1;
        let mut details = details(context);
        details["config_version"] = json!(context.snapshot.version);
        details["route"] = json!(context.route.as_ref().map(|route| &route.spec.name));
        details["actor"] = json!(context.actor.as_ref().map(|actor| hex(actor)));
        let event = Event {
            request_id: context.id.clone(), sequence: context.sequence, stage: stage.into(), timestamp_ms: now_ms(),
            elapsed_ms: context.started.elapsed().as_millis() as u64, details,
        };
        if context.capture && (context.snapshot.config.telemetry.capture != "summary" || matches!(stage,
            "inspected"|"rejected"|"completed"|"blocked"|"failed"|"analyzed"|"analysis_skipped"))
            && context.snapshot.config.control.enabled && context.snapshot.config.telemetry.enabled { self.telemetry.publish(event.clone()); }
        if let Some(journey)=&context.journey { journey.lock().unwrap_or_else(|error|error.into_inner()).push(event); }
        else if context.capture && context.events.len()<64 {context.events.push(event);}

    }

    pub(super) fn sample ( context: &mut Context, bytes: &[u8] ) {
        if context.background.is_none() { return; }
        let remaining=context.snapshot.config.model.scan_bytes.saturating_sub(context.sample.len());
        context.sample.extend_from_slice(&bytes[..bytes.len().min(remaining)]);
        context.sample_seen=context.sample_seen.saturating_add(bytes.len());
    }

    pub(super) fn response_identity ( header: &mut ResponseHeader, context: &Context ) -> pingora::Result<()> {

        let identity = &context.snapshot.config.identity;
        let names = &context.snapshot.identity_headers;
        header.remove_header("x-request-id");
        if names.request_id != "x-request-id" { header.remove_header(&names.request_id); }
        if identity.propagate {
            if let Some(value)=&context.id_value {header.insert_header(names.request_id.clone(),value.clone())?;}
            else {header.insert_header(names.request_id.clone(),&context.id)?;}
        }
        Ok(())

    }

    pub(super) async fn respond ( session: &mut Session, context: &Context, code: u16 ) -> pingora::Result<()> {

        session.set_keepalive(None);
        let mut header = ResponseHeader::build(code, Some(3))?;
        header.insert_header("content-length", "0")?;
        Self::response_identity(&mut header, context)?;
        header.insert_header("cache-control", "no-store")?;
        if code == 429 { header.insert_header("retry-after", "10")?; }
        session.write_response_header(Box::new(header), true).await

    }

    pub(super) async fn reject ( &self, session: &mut Session, context: &mut Context, code: u16, reason: &str ) -> pingora::Result<bool> {

        context.blocked = true;
        self.record(context, "rejected", |_| json!({ "status": code, "reason": reason }));
        if !context.inspection.denial_cached && let (Some(actor), Some(route)) = (context.actor, &context.route) {
            self.hook("blocked", &context.id, &route.spec.name, &actor, reason);
        }
        Self::respond(session, context, code).await?;
        Ok(true)

    }

}

impl Context {

    pub(super) fn new ( snapshot: Arc<Snapshot> ) -> Self {

        let started=Instant::now();
        Self {
            id_value: None, id: Uuid::new_v4().to_string(), started,
            capture: snapshot.config.store.is_some() || (snapshot.config.control.enabled && snapshot.config.telemetry.enabled),
            wait_deadline: None, waited: false,
            snapshot, route: None, canonical: None, lease: None, excluded: [0;3], attempts: 0, upstream_started: started, permit: None,
            actor: None, peer: None, inspection: Inspection::default(), ticket: None,
            sample: Vec::new(), sample_seen: 0, response_sample: Vec::new(), response_seen: 0, response_available: false, forwarded: false, generation: 0, storage: None, analysis_slot: None, events: Vec::new(), journey: None,
            sequence: 0, blocked: false, model_rejected: false, request_bytes: 0, background: None,
            cache_key: None, fill: None, cache_hit: false,
        }

    }

}
