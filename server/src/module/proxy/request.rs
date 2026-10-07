use std::time::Duration;
use pingora::proxy::Session;
use serde_json::json;

use crate::core::domain::Facts;
use crate::module::{config::Mode, identity::Identity, runtime::canonical_path};
use super::arch::{Context, Proxy};

impl Proxy {

    pub(super) async fn admit ( &self, session: &mut Session, context: &mut Context ) -> pingora::Result<bool> {

        let (ticket, sampled) = self.telemetry.begin(&context.snapshot.config.telemetry,context.snapshot.config.control.enabled);
        context.ticket = ticket;
        context.peer = session.client_addr().and_then(|address| address.as_inet()).map(|address| address.ip());
        if let Some(peer) = context.peer {
            let identity = &context.snapshot.config.identity;
            if identity.preserve_trusted_id && Identity::trusted(identity, peer)
                && let Some(id) = session.req_header().headers.get(&identity.request_id_header).and_then(|value| value.to_str().ok())
                && let Ok(id) = uuid::Uuid::parse_str(id)
            { context.id = id.to_string(); }
        }
        let path = canonical_path(session.req_header().uri.path());
        if let Some(path) = &path {
            let host = session.req_header().headers.get("host").and_then(|value| value.to_str().ok()).unwrap_or("");
            context.route = context.snapshot.route(host, path, session.req_header().method.as_str(), session.req_header());
        }
        if let Some(route) = &context.route { context.capture = route.policy.capture; }
        let config = &context.snapshot.config;
        let tracks_actor = config.webhooks.enabled || config.identity.backend_block_header.is_some()
            || context.route.as_ref().is_some_and(|route| route.policy.capture || route.policy.mode != Mode::Off
                || config.limits.rate_limit_10s > 0 || route.spec.rate_limit_10s.unwrap_or(0) > 0
                || crate::module::decision::Engine::cache_enabled(&context.snapshot, route));
        if tracks_actor && let Some(peer) = context.peer {
            let identity = &config.identity;
            let claimed = identity.actor_header.as_ref().and_then(|name| session.req_header().headers.get(name)).and_then(|value| value.to_str().ok());
            context.actor = Some(self.identity.actor(identity, peer, claimed));
        }
        context.capture &= sampled;
        if config.control.enabled && (context.capture || context.route.as_ref().is_some_and(|route| route.policy.mode != Mode::Off)) {
            context.journey = self.journeys.begin(&mut context.id,
                context.route.as_ref().map_or_else(String::new,|route|route.spec.name.clone()),
                context.actor.as_ref().map_or_else(String::new,|actor|crate::core::domain::hex(actor)), context.capture);
        }
        if config.control.enabled && context.capture && self.store.enabled() && context.journey.is_none() && config.persistence.admission=="required" {
            return self.reject(session,context,503,"journey_capacity").await;
        }
        if context.snapshot.config.identity.propagate {
            context.id_value=http::HeaderValue::from_str(&context.id).ok();
        }
        if context.capture && self.store.enabled() {
            let required=context.snapshot.config.persistence.admission=="required";
            context.storage=self.store.try_reserve();
            if required && context.storage.is_none() {
                context.storage=self.wait_for(context,"storage",self.store.wait_reserve()).await;
            }
            if context.storage.is_none() {
                self.store.reservation_failed(required);
                if required {return self.reject(session,context,503,"storage_capacity").await;}
            }
        }
        context.generation = self.engine.verdicts.as_ref().map_or(0,|store|store.generation());
        self.record(context, "received", |context| json!({
            "method": match session.req_header().method.as_str() {
                value @ ("GET" | "HEAD" | "POST" | "PUT" | "PATCH" | "DELETE" | "OPTIONS" | "CONNECT" | "TRACE") => value, _ => "OTHER",
            },
            "peer_ip": context.peer.map(|peer| peer.to_string()),
        }));
        if path.is_none() { return self.reject(session, context, 400, "ambiguous_path").await; }
        context.canonical=path.and_then(|value|match value {std::borrow::Cow::Owned(path)=>Some(path),std::borrow::Cow::Borrowed(_)=>None});
        let Some(route) = context.route.clone() else { return self.reject(session, context, 404, "no_route").await; };
        if route.spec.deny { return self.reject(session, context, 403, "route_denied").await; }
        let timeout = Some(Duration::from_millis(route.policy.limits.timeout_ms));
        session.set_read_timeout(timeout);
        session.set_write_timeout(timeout);
        if session.get_keepalive().is_some() {
            let seconds = context.snapshot.config.runtime.keepalive_seconds;
            session.set_keepalive((seconds > 0).then_some(seconds));
        }
        if session.req_header().method.as_str() == "CONNECT" || session.req_header().headers.contains_key("upgrade") {
            return self.reject(session, context, 501, "protocol_not_supported").await;
        }
        let request=session.req_header();
        if request.uri.path().len() + request.uri.query().map_or(0,str::len) > 8192 || request.headers.len() > 100 {
            return self.reject(session, context, 431, "request_metadata_limit").await;
        }
        let length=request.headers.get("content-length").and_then(|value|value.to_str().ok()).and_then(|value|value.parse::<u64>().ok()).unwrap_or(0);
        if length > route.policy.limits.max_body_bytes as u64 {
            return self.reject(session, context, 413, "request_body_limit").await;
        }
        if context.peer.is_none() { return self.reject(session, context, 400, "missing_network_peer").await; }
        context.permit = self.capacity.try_acquire();
        if context.permit.is_none() {
            context.permit=self.wait_for(context,"request",async {Some(self.capacity.acquire().await)}).await;
            if context.permit.is_none() {return self.reject(session,context,503,"request_capacity").await;}
        }
        let Some(actor) = context.actor else { return self.cached_response(session, context).await; };
        let facts=Self::facts(session,length);
        context.inspection = self.engine.inspect(&facts, actor, &context.snapshot, &route, route.policy.capture).await;
        context.model_rejected = context.inspection.model_rejected;
        if route.policy.mode != Mode::Off {
            context.background = context.inspection.features;
            Self::sample(context,session.req_header().method.as_str().as_bytes());
            Self::sample(context,b" ");
            Self::sample(context,session.req_header().uri.path_and_query().map_or(b"/".as_slice(),|value|value.as_str().as_bytes()));
            Self::sample(context,b"\n");
        }
        self.record(context, "inspected", |context| json!({
            "decision": if context.inspection.status.is_some() { "deny" } else { "allow" },
            "reason": context.inspection.reason,
            "cached": context.inspection.cached,
            "model_state": context.inspection.model_state,
            "risk_score": context.inspection.score, "threshold": route.policy.threshold,
            "content_threshold": route.policy.content_threshold, "journey_threshold": route.policy.journey_threshold,
            "features": context.inspection.features, "feature_version": crate::module::inference::FEATURE_VERSION, "features_normalized": false,
            "model_version": self.engine.model_info.as_ref().map(|info| &info.model_version),
            "model_sha256": self.engine.model_info.as_ref().map(|info| &info.artifact_sha256),
            "model_source": self.engine.model_info.as_ref().map(|info| &info.source),
        }));
        if context.inspection.status.is_none() && context.inspection.score.is_some_and(|score| score >= route.policy.threshold) {
            self.hook("risk_detected", &context.id, &route.spec.name, &actor, "observed_threshold");
        }
        if let Some(status) = context.inspection.status {
            let reason = context.inspection.reason.clone();
            return self.reject(session, context, status, &reason).await;
        }

        if route.policy.mode != Mode::Off && context.background.is_some() && context.snapshot.config.model.on_overload=="reject" {
            if let Some(worker)=&self.engine.inference {
                context.analysis_slot=worker.try_reserve();
                if context.analysis_slot.is_none() { context.analysis_slot=self.wait_for(context,"analysis",worker.wait_reserve()).await; }
                if context.analysis_slot.is_none() {worker.reservation_failed();}
            }
            if context.analysis_slot.is_none() { return self.reject(session,context,503,"analysis_capacity").await; }
        }
        if let Some((status,reason))=self.queued_restriction(context).await {
            return self.reject(session,context,status,&reason).await;
        }
        self.cached_response(session, context).await

    }

    fn facts ( session: &Session, length: u64 ) -> Facts {

        let request = session.req_header();
        Facts {
            read: matches!(request.method.as_str(), "GET" | "HEAD" | "OPTIONS"),
            write: matches!(request.method.as_str(), "POST" | "PUT" | "PATCH" | "DELETE"),
            path_length: request.uri.path().len(), query_length: request.uri.query().map_or(0, str::len),
            header_count: request.headers.len(), declared_body: length,
            has_body: length > 0 || request.headers.contains_key("transfer-encoding"),
            path_depth: request.uri.path().split('/').filter(|part| !part.is_empty()).count(),
        }

    }

}
