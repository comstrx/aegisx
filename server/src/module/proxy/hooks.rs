use std::time::{Duration, Instant};

use async_trait::async_trait;
use bytes::Bytes;
use pingora::http::{RequestHeader, ResponseHeader};
use pingora::prelude::HttpPeer;
use pingora::proxy::{FailToProxy, ProxyHttp, Session};
use pingora::{Error, ErrorType};
use serde_json::json;

use super::arch::{Context, Proxy};

#[async_trait]
impl ProxyHttp for Proxy {

    type CTX = Context;

    fn new_ctx ( &self ) -> Context { Context::new(self.current.load_full()) }

    async fn request_filter ( &self, session: &mut Session, context: &mut Context ) -> pingora::Result<bool> {

        self.admit(session, context).await

    }

    async fn upstream_peer ( &self, _session: &mut Session, context: &mut Context ) -> pingora::Result<Box<HttpPeer>> {

        let route = context.route.as_ref().expect("admitted route").clone();
        let pool = route.pool.as_ref().expect("admitted upstream pool");
        if context.attempts>=context.excluded.len() {return Err(Error::explain(ErrorType::HTTPStatus(502),"Connect retry limit reached"));}
        context.lease = pool.select(self.started.elapsed().as_millis() as u64, &context.excluded[..context.attempts]);
        if context.lease.is_none() {
            let excluded=context.excluded;
            let attempts=context.attempts;
            context.lease=self.wait_for(context,"upstream",pool.wait_select(self.started,&excluded[..attempts])).await;
        }
        let Some(lease) = &context.lease else {
            return Err(Error::explain(ErrorType::HTTPStatus(503), "No healthy upstream capacity"));
        };
        context.excluded[context.attempts]=lease.index;
        context.attempts+=1;
        if pool.measure_latency { context.upstream_started = Instant::now(); }
        let mut peer = lease.backend.peer.clone();
        peer.group_key=crate::module::upstream::reuse_group(context.snapshot.config.runtime.work_stealing);
        let timeout = Some(Duration::from_millis(route.policy.limits.timeout_ms));
        peer.options.connection_timeout = timeout;
        peer.options.total_connection_timeout = timeout;
        peer.options.read_timeout = timeout;
        peer.options.write_timeout = timeout;
        peer.options.idle_timeout = Some(Duration::from_secs(context.snapshot.config.runtime.pool_idle_seconds));
        self.record(context, "upstream_selected", |context| json!({
            "address": context.lease.as_ref().map(|lease| lease.backend.config.address),
            "pool": context.route.as_ref().map(|route| &route.spec.upstream),
            "attempt": context.attempts,
        }));

        Ok(Box::new(peer))

    }

    fn fail_to_connect ( &self, _session: &mut Session, _peer: &HttpPeer, context: &mut Context, mut error: Box<Error> ) -> Box<Error> {

        if let Some(lease) = context.lease.take() { lease.finish(self.started.elapsed().as_millis() as u64, true); }
        let route = context.route.as_ref().expect("admitted route");
        let pool = route.pool.as_ref().expect("admitted upstream pool");
        error.set_retry(context.attempts < pool.config.options.connect_attempts);
        self.record(context, "connect_failed", |context| json!({ "attempt": context.attempts }));

        error

    }

    fn error_while_proxy ( &self, _peer: &HttpPeer, _session: &mut Session, mut error: Box<Error>, _context: &mut Context, _client_reused: bool ) -> Box<Error> {

        // Once a connection is established, upstream side effects cannot be ruled out.
        error.set_retry(false);
        error

    }

    async fn upstream_request_filter ( &self, _session: &mut Session, request: &mut RequestHeader, context: &mut Context ) -> pingora::Result<()> {

        if let Some((status,reason))=self.queued_restriction(context).await {
            context.blocked=true;
            self.record(context,"rejected", |_|json!({"status":status,"reason":reason,"forwarding_started":false}));
            return Err(Error::explain(ErrorType::HTTPStatus(status),"Queued request restricted before forwarding"));
        }
        self.prepare(request, context)?;
        context.forwarded = true;
        self.record(context, "upstream_request_prepared", |_| json!({}));

        Ok(())

    }

    async fn request_body_filter ( &self, _session: &mut Session, body: &mut Option<Bytes>, end: bool, context: &mut Context ) -> pingora::Result<()> {

        if let Some(body) = body { Self::sample(context,body); }
        context.request_bytes = context.request_bytes.saturating_add(body.as_ref().map_or(0, Bytes::len));
        let limit = context.route.as_ref().map_or(context.snapshot.config.limits.max_body_bytes, |route| route.policy.limits.max_body_bytes);
        if context.request_bytes > limit {
            context.blocked = true;
            self.record(context, "rejected", |_| json!({ "status": 413, "reason": "streamed_body_limit", "partial_forwarding_possible": true }));
            return Err(Error::explain(ErrorType::HTTPStatus(413), "Request body exceeds configured limit"));
        }
        if end { self.record(context, "request_body_complete", |context| json!({ "bytes": context.request_bytes })); }

        Ok(())

    }

    async fn response_filter ( &self, _session: &mut Session, response: &mut ResponseHeader, context: &mut Context ) -> pingora::Result<()> {

        if context.route.as_ref().and_then(|route|route.pool.as_ref()).is_some_and(|pool|pool.measure_latency)
            && let Some(lease) = &context.lease { lease.latency(context.upstream_started.elapsed().as_micros() as u64); }
        if context.background.is_some() && context.snapshot.config.model.response_scan_bytes>0 {
            let kind=response.headers.get("content-type").and_then(|value|value.to_str().ok())
                .unwrap_or("").split(';').next().unwrap_or("").trim().to_ascii_lowercase();
            let identity=response.headers.get_all("content-encoding").iter()
                .all(|value|value.to_str().is_ok_and(|value|value.eq_ignore_ascii_case("identity")));
            context.response_available=identity && (kind.starts_with("text/") || matches!(kind.as_str(),
                "application/json"|"application/xml"|"application/x-www-form-urlencoded") || kind.ends_with("+json") || kind.ends_with("+xml"));
        }
        if let Some(route) = &context.route {
            for (name, value) in &route.policy.response_headers { response.insert_header(name.clone(), value.clone())?; }
        }
        let mut feedback = false;
        if let Some(name) = &context.snapshot.config.identity.backend_block_header {
            if let Some(seconds) = response.headers.get(name).and_then(|value| value.to_str().ok()).and_then(|value| value.parse::<u64>().ok())
                && seconds > 0 && let (Some(actor), Some(route)) = (context.actor, &context.route)
            {
                feedback = true;
                if crate::module::decision::Engine::cache_enabled(&context.snapshot, route) {
                    let key = crate::module::decision::Engine::key(&context.snapshot, route, &actor);
                    if let Some(repository) = &self.engine.verdicts {
                        let now=crate::core::time::now_ms();
                        let verdict=crate::module::verdict::Verdict { key:crate::core::domain::hex(&key),
                            actor:crate::core::domain::hex(&actor),route:route.spec.name.clone(),reason:"backend_signal".into(),
                            source:"backend".into(),created_ms:now,expires_ms:now+seconds.saturating_mul(1000).min(context.snapshot.config.cache.deny_ttl_ms),
                            request_id:Some(context.id.clone()) };
                        if repository.submit(key,verdict,None).is_err() { tracing::warn!("Backend verdict queue full"); }
                    }
                }
                self.hook("backend_signal", &context.id, &route.spec.name, &actor, "backend_requested_block");
            }
            response.remove_header(name);
        }
        Self::response_identity(response, context)?;
        if !feedback && let Some(key) = context.cache_key { context.fill = self.caches.fill(key, response, &context.snapshot.config.cache); }
        self.record(context, "response_headers", |_| json!({ "status": response.status.as_u16() }));

        Ok(())

    }

    fn response_body_filter ( &self, _session: &mut Session, body: &mut Option<Bytes>, _end: bool, context: &mut Context ) -> pingora::Result<Option<Duration>> {
        if context.response_available && let Some(body)=body.as_ref() {
            let remaining=context.snapshot.config.model.response_scan_bytes.saturating_sub(context.response_sample.len());
            context.response_sample.extend_from_slice(&body[..body.len().min(remaining)]);
            context.response_seen=context.response_seen.saturating_add(body.len());
        }
        if let Some(fill) = &mut context.fill && !fill.append(body.as_ref()) { context.fill = None; }
        Ok(None)
    }

    async fn fail_to_proxy ( &self, session: &mut Session, error: &Error, context: &mut Context ) -> FailToProxy {

        self.failure(session, error, context).await

    }

    async fn logging ( &self, session: &mut Session, error: Option<&Error>, context: &mut Context ) {

        self.complete(session, error, context);

    }

    fn request_summary ( &self, _session: &Session, context: &Context ) -> String {

        format!("request_id={}", context.id)

    }

}
