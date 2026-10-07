use pingora::{Error, ErrorSource, proxy::Session};
use serde_json::json;
use crate::module::{inference::Envelope, services::AnalysisContext};
use super::{Context, Proxy};

impl Proxy {
    fn records ( context: &mut Context ) -> Vec<crate::module::storage::Event> {
        if !context.capture { return Vec::new(); }
        let mut events=if let Some(handle)=&context.journey {
            std::mem::take(&mut handle.lock().unwrap_or_else(|error|error.into_inner()).events)
        } else {std::mem::take(&mut context.events)};
        if context.snapshot.config.telemetry.capture=="summary" {
            events.retain(|event|matches!(event.stage.as_str(),
                "inspected"|"rejected"|"completed"|"blocked"|"failed"|"analyzed"|"analysis_skipped"|"backend_reported"));
        }
        events
    }
    fn persist ( &self, context: &mut Context ) {
        if !context.capture || !self.store.enabled() { return; }
        let events=Self::records(context);
        if events.is_empty() { return; }
        if let Some(reservation)=context.storage.take() { reservation.commit(events); }
    }
    pub(super) fn complete ( &self, session: &Session, error: Option<&Error>, context: &mut Context ) {
        let status = session.response_written().map(|response| response.status.as_u16());
        let elapsed = if context.capture || context.journey.is_some() || context.ticket.is_some() || context.background.is_some() {
            context.started.elapsed().as_millis() as u64
        } else {0};
        let failed = !context.model_rejected && (error.is_some() || status.is_some_and(|code| code >= 400));
        if let Some(mut guard) = context.inspection.guard.take() { guard.finish(failed, context.blocked && !context.model_rejected); }
        if let Some(mut ticket) = context.ticket.take() {
            ticket.finish(error.is_some() || status.is_some_and(|code| code >= 500), context.blocked);
            self.telemetry.observe(elapsed,session.body_bytes_read() as u64,session.body_bytes_sent() as u64);
        }
        if let Some(lease) = context.lease.take() {
            if error.is_some_and(|error| error.esource() == &ErrorSource::Upstream) { lease.finish(self.started.elapsed().as_millis() as u64, true); }
            else if error.is_none() {
                let failed=status.is_some_and(|code| code >= 500);
                lease.finish(if failed {self.started.elapsed().as_millis() as u64} else {0}, failed);
            }
        }
        if let Some(fill) = context.fill.take() && error.is_none() && status == Some(200) {
            self.caches.save(fill, &context.snapshot.config.identity.request_id_header);
        }
        if context.snapshot.config.cache.responses && error.is_none() && status.is_some_and(|code| (200..400).contains(&code))
            && !matches!(session.req_header().method.as_str(), "GET" | "HEAD" | "OPTIONS" | "TRACE")
        { self.caches.purge_responses(); }
        context.permit.take();
        let stage = if error.is_some() { "failed" } else if context.blocked { "blocked" } else { "completed" };
        self.record(context, stage, |context| json!({
            "status":status,"request_bytes":session.body_bytes_read(),"response_bytes":session.body_bytes_sent(),
            "error_kind":error.map(|error|format!("{:?}",error.etype())),"response_committed":status.is_some(),
            "response_cache":context.cache_hit,"forwarding_started":context.forwarded,
        }));
        if context.journey.is_some() { self.journeys.finish(&context.id); }
        if context.blocked || context.cache_hit || !context.forwarded || context.background.is_none() {
            self.persist(context); return;
        }
        let (Some(admission),Some(actor),Some(route)) = (context.background.take(),context.actor,context.route.clone()) else { return; };
        let (backend_events,events_truncated) = context.journey.as_ref().map(|handle| {
            let mut journey=handle.lock().unwrap_or_else(|error|error.into_inner());
            (if context.snapshot.config.model.journey {std::mem::take(&mut journey.backend_events)} else {Vec::new()},journey.truncated)
        }).unwrap_or_default();
        let envelope = Envelope { journal:context.storage.is_some(),cache_scores:context.snapshot.config.cache.scores,admission,sample:std::mem::take(&mut context.sample),sample_seen:context.sample_seen,
            outcome:[status.unwrap_or(0) as f32,elapsed as f32,session.body_bytes_read() as f32,session.body_bytes_sent() as f32,
                context.attempts as f32,f32::from(error.is_some()),1.0,1.0], request_id:context.id.clone(),backend_events,events_truncated,
            response_sample:std::mem::take(&mut context.response_sample),response_seen:context.response_seen,response_available:context.response_available };
        let work = AnalysisContext { snapshot:context.snapshot.clone(),route,actor,request_id:context.id.clone(),
            sequence:context.sequence+1,generation:context.generation,capture:context.capture,elapsed_ms:elapsed,
            storage:context.storage.take(),records:Self::records(context),slot:context.analysis_slot.take() };
        if !self.services.analyze(envelope,work) {
            self.record(context,"analysis_skipped", |_|json!({"reason":"analysis_queue_full","completed_request_unchanged":true}));
            self.persist(context);
        }
    }
}
