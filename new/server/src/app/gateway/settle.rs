use std::io::Write;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Instant;

use http::Method;
use http::header::{CACHE_CONTROL, CONNECTION, CONTENT_DISPOSITION, CONTENT_TYPE, EXPIRES, HOST, HeaderMap, HeaderName, HeaderValue, SET_COOKIE};
use http_body::Body as _;
use serde_json::json;

use crate::app::{Completion, Identity, Outcome, Pending, Snapshot, Store, Trace};
use crate::core::error::AppError;
use crate::core::log::{debug, warn};
use crate::core::net::Address;
use crate::core::time::Clock;
use crate::http::body::{Body, Tap};
use crate::http::encode::Replaced;
use crate::http::files::Fetch;
use crate::http::header::{Header, Rendered, Var};
use crate::http::proxy::{Plan, Proxy};
use crate::http::request::{Req, Request, Shadow};
use crate::http::response::{Res, Response};
use crate::http::upstream::{Failure, Tunnel};
use super::arch::{Context, Flight, Flow, Handler, Hop, Landed, Ticket, Vars};

impl Handler {

    pub(super) fn land <'p> ( &self, flight: &mut Flight<'_>, mut hop: Hop<'p>, outcome: Result<( Res<Body>, Option<Shadow> ), Failure>, flow: Flow, excluded: &mut Vec<usize> ) -> Landed<'p> {

        let ( pool, backend ) = ( hop.pool, hop.backend );

        match outcome {
            Ok(( mut response, again )) => {

                Proxy::settle(&mut response, &flight.route.plan, &backend.upstream);

                let status = response.status().as_u16();

                if flow.more && flow.retryable && pool.retry.status(status) && pool.backends.iter().any(|other| other.index != backend.index && !excluded.contains(&other.index) && other.available(flow.now)) && let Some(again) = again.as_ref().and_then(Shadow::restore) {

                    backend.fail(pool, flow.now);
                    excluded.push(backend.index);

                    if let Some(counts) = backend.count(self.worker) { counts.served.add(1); counts.retried.add(1); }

                    if let Some(trace) = flight.trace() { trace.record("retry", json!({ "backend": backend.addr.to_string(), "status": status })); }

                    return Err(( Some(again), None ));

                }

                let rescued = matches!(status, 500 | 502 | 503 | 504) && Self::rescue(flight, flow.head).map(|stale| response = stale).is_some();

                hop.elapsed_us = Clock::elapsed_us(flight.started);

                if !rescued {

                    match pool.slow_us > 0 && hop.elapsed_us > pool.slow_us { true => { backend.fail(pool, flow.now); } false => backend.succeed() }

                }

                self.receive(flight, &hop, &mut response, flow.head);

                let buffer = flight.route.plan.buffer_response > 0 && response.status() != http::StatusCode::SWITCHING_PROTOCOLS;

                Ok(( response, hop, buffer ))

            }
            Err(failure) => {

                let status = failure.error.status();
                let timeout = matches!(failure.error, AppError::Timeout { .. });
                let again = flow.more && pool.retry.failure(failure.connect, timeout);

                if (failure.connect || timeout || matches!(failure.error, AppError::Network { .. })) && backend.fail(pool, flow.now) { warn!(pool = %pool.name, backend = %backend.addr, "backend quarantined after repeated failures"); }

                if failure.connect || again { excluded.push(backend.index); }

                if let Some(counts) = backend.count(self.worker) { counts.failed.add(1); if again { counts.retried.add(1); } }

                debug!(peer = %flight.context.peer.addr, backend = %backend.addr, error = %failure.error, "forward failed");

                if let Some(trace) = flight.trace() { trace.record("upstream_failed", json!({ "backend": backend.addr.to_string(), "reason": failure.error.to_string() })); }

                let request = match ( again && flow.retryable || again && failure.connect, failure.request ) {
                    ( true, request ) => request,
                    ( false, Some(request) ) => { Self::discard((*request).into_body(), flight.route.plan.max_body_bytes); None }
                    ( false, None ) => None,
                };

                Err(( request, Some(status) ))

            }
        }

    }

    pub(super) async fn buffered ( &self, mut flight: Flight<'_>, response: Res<Body>, hop: Hop<'_> ) -> Res<Body> {

        let ( head, body ) = response.into_parts();

        match Body::prefix(body, flight.route.plan.buffer_response).await {
            Ok(body) => self.complete(flight, Res::from_parts(head, body), hop),
            Err(error) => {

                let trace = flight.traced();

                self.refuse(error.status(), flight.observe, flight.started, trace)

            }
        }

    }

    fn receive ( &self, flight: &mut Flight<'_>, hop: &Hop<'_>, response: &mut Res<Body>, head: bool ) {

        let route = flight.route;

        if let Some(counts) = hop.backend.count(self.worker) { counts.served.add(1); }

        self.picker.with_mut(|picker| picker.observe(hop.backend, hop.pool.index, hop.elapsed_us));

        if response.status() == http::StatusCode::NOT_MODIFIED && let Some(store) = &self.cache && let Some(rare) = flight.rare.as_deref_mut() && let Some(entry) = rare.revalidating.take() && let Some(key) = rare.filling.take() {

            *response = store.renew(key, &entry, response.headers(), head, Clock::wall_ms(flight.started));

        }

        let status = response.status().as_u16();

        if let Some(actor) = flight.actor() { actor.finish(status, hop.elapsed_us / 1_000, false); }

        if let Some(name) = &flight.runtime.snapshot.names.block && let Some(value) = response.headers_mut().remove(name) {

            let seconds = value.to_str().ok().and_then(|text| text.trim().parse::<u64>().ok()).filter(|seconds| *seconds > 0);

            if let ( Some(seconds), Some(decisions), Some(( key, _, _ )) ) = ( seconds, &self.decisions, flight.watched() ) && route.policy.decisions {

                let ( decision, verdict ) = decisions.verdict(&route.name, key, seconds.saturating_mul(1_000), "backend signal", "backend", flight.id.to_str().ok().map(str::to_owned));

                drop(decisions.block(decision, verdict));

            }

        }

        if status == 101 {

            match ( flight.rare.as_deref_mut().and_then(|rare| rare.upgrading.take()), response.body_mut().take_upstream() ) {
                ( Some(upgrading), Some(streaming) ) => {

                    response.headers_mut().insert(CONNECTION, HeaderValue::from_static("upgrade"));
                    Tunnel::bridge(upgrading, *streaming, route.plan.timeout_ms, Some(Rc::new(self.bridged())));

                }
                _ => { *response.body_mut() = Body::Empty; }
            }

        }

    }

    pub(super) async fn relocate ( &self, flight: Flight<'_>, upstream: Res<Body>, hop: Hop<'_>, target: HeaderValue ) -> Res<Body> {

        let snapshot = &flight.runtime.snapshot;
        let host = flight.route.spec.host.as_deref().unwrap_or("");
        let blank = HeaderMap::new();
        let path = target.to_str().ok().and_then(Request::canonical);

        let found = path.as_deref().and_then(|path| snapshot.route(host, path, &Method::GET, &blank))
            .filter(|route| route.spec.internal)
            .and_then(|route| route.files.as_ref().map(|files| ( route, files )));

        let ( Some(( route, files )), Some(path) ) = ( found, path.as_deref() ) else { return self.complete(flight, Response::status(404), hop); };

        let mut response = Box::pin(files.serve(&self.files, Fetch { method: &Method::GET, headers: &blank, path, strip: route.plan.mount.len(), query: None, now_ms: Clock::wall_ms(flight.started) })).await;

        for name in [CONTENT_TYPE, CONTENT_DISPOSITION, CACHE_CONTROL, EXPIRES, SET_COOKIE] {

            let values: Vec<HeaderValue> = upstream.headers().get_all(&name).iter().cloned().collect();

            if values.is_empty() { continue; }

            response.headers_mut().remove(&name);

            for value in values { response.headers_mut().append(&name, value); }

        }

        self.complete(flight, response, hop)

    }

    pub(super) fn complete ( &self, mut flight: Flight<'_>, mut response: Res<Body>, hop: Hop<'_> ) -> Res<Body> {

        let route = flight.route;
        let snapshot = &flight.runtime.snapshot;
        let backend = hop.backend;
        let status = response.status().as_u16();

        Identity::echo(response.headers_mut(), snapshot, &flight.id);

        if flight.gunzip { snapshot.compression.gunzip(&mut response); }

        if let Some(swaps) = &route.replace { Replaced::apply(swaps, &mut response); }

        if let Some(vars) = flight.rare.as_deref().and_then(|rare| rare.vars.as_ref()) && route.plan.dynamic_response() { for ( name, value ) in Self::render(&route.plan.response_headers, flight.context, vars, &flight.id, snapshot, Some(&backend.addr)) { response.headers_mut().insert(name, value); } }

        if let Some(sticky) = &hop.pool.sticky && hop.stuck != Some(backend.index) { sticky.issue(response.headers_mut(), backend); }

        if let Some(charset) = &flight.route.spec.charset { Self::charset(&mut response, charset); }

        if snapshot.config.hooks.response { self.answered(&flight, &mut response); }

        let response_bytes = response.body().size_hint().exact().unwrap_or(0);

        if flight.observe { self.stats.finish(Outcome::of(status), hop.elapsed_us / 1_000, flight.declared, response_bytes); }

        if let Some(trace) = flight.traced() { trace.finish("completed", json!({ "status": status, "backend": backend.addr.to_string(), "attempts": hop.attempt + 1 })); }

        let Some(mut ticket) = flight.ticket.take() else { return response; };

        if let Some(analyser) = &self.analyser && let Some(probing) = flight.rare.as_deref_mut().and_then(|rare| rare.probe.take()) {

            let ( admission, request_tap, response_tap ) = *probing;

            response.body_mut().probe(response_tap.clone());

            ticket.pending = Some(Box::new(Pending {
                analyser  : analyser.clone(),
                capture   : self.capture.clone(),
                worker    : self.worker,
                id        : Arc::from(flight.id.to_str().unwrap_or("")),
                route     : route.name.clone(),
                admission,
                request   : request_tap,
                response  : response_tap,
                started   : flight.started,
                outcome   : Completion { status, elapsed_ms: hop.elapsed_us / 1_000, request_bytes: flight.declared, response_bytes, attempts: hop.attempt as u32 + 1, failed: false },
            }));

        }

        if let Some(store) = &self.cache && let Some(key) = flight.rare.as_deref_mut().and_then(|rare| rare.filling.take()) && let Some(ttl) = store.storable(status, response.headers(), Clock::wall_ms(flight.started)) && store.admit(&key, response.headers()) {

            let tap = match &ticket.pending { Some(pending) => pending.response.clone(), None => { let tap = Tap::new(store.max_object); response.body_mut().probe(tap.clone()); tap } };

            let claim = flight.rare.as_deref_mut().and_then(|rare| rare.claim.take());

            ticket.fill = Some(Box::new(store.fill(key, status, response.headers(), ttl, tap, Clock::wall_ms(flight.started)).claimed(claim)));

        }

        let compressed = flight.encoding.is_some_and(|encoding| snapshot.compression.apply(encoding, &mut response));

        if let ( Some(mut entry), Some(log) ) = ( flight.ledger.take(), &self.access ) {

            let counter = match ( &ticket.pending, compressed ) {
                ( Some(pending), false ) => pending.response.clone(),
                _ => { let counter = Tap::new(0); response.body_mut().probe(counter.clone()); counter }
            };

            entry.status = status;
            entry.backend = Some(backend.addr.clone());
            entry.header_us = hop.elapsed_us;
            entry.attempts = hop.attempt as u8 + 1;
            entry.sent = Some(counter);
            ticket.access = Some(( log.clone(), entry ));

        }

        ticket.lease = hop.lease;
        flight.served.proxied = true;
        Self::attach(&mut response, ticket);

        response

    }

    fn rescue ( flight: &mut Flight<'_>, head: bool ) -> Option<Res<Body>> {

        let rare = flight.rare.as_deref_mut()?;
        let entry = rare.fallback.take()?;

        rare.filling = None;
        rare.revalidating = None;
        flight.served.proxied = true;

        Some(Store::respond(&entry, if head { &Method::HEAD } else { &Method::GET }, &HeaderMap::new(), Clock::wall_ms(flight.started), true))

    }

    pub(super) fn fail ( &self, mut flight: Flight<'_>, status: u16 ) -> Res<Body> {

        if let Some(mut response) = Self::rescue(&mut flight, false) {

            Identity::echo(response.headers_mut(), &flight.runtime.snapshot, &flight.id);

            if flight.observe { self.stats.finish(Outcome::Completed, Clock::elapsed_ms(flight.started), flight.declared, response.body().size_hint().exact().unwrap_or(0)); }

            if let Some(trace) = flight.traced() { trace.finish("cached", json!({ "status": response.status().as_u16(), "stale": true })); }

            return response;

        }

        let mut response = Response::status(status);

        Identity::echo(response.headers_mut(), &flight.runtime.snapshot, &flight.id);

        if let Some(actor) = flight.actor() { actor.finish(status, Clock::elapsed_ms(flight.started), false); }

        if flight.observe { self.stats.finish(Outcome::Failed, Clock::elapsed_ms(flight.started), flight.declared, 0); }

        if let Some(trace) = flight.traced() { trace.finish("failed", json!({ "status": status })); }

        response

    }

    pub(super) fn deliver ( &self, mut response: Res<Body>, request: Box<Req<Body>>, mut flight: Flight<'_> ) -> Res<Body> {

        let snapshot = &flight.runtime.snapshot;
        let status = response.status().as_u16();

        Self::decorate(&mut response, &flight.route.plan, flight.context, &request, &flight.id, snapshot);

        if let Some(charset) = &flight.route.spec.charset { Self::charset(&mut response, charset); }

        if snapshot.config.hooks.response { self.answered(&flight, &mut response); }

        if !flight.served.proxied && let Some(swaps) = &flight.route.replace { Replaced::apply(swaps, &mut response); }

        if let Some(encoding) = flight.encoding { snapshot.compression.apply(encoding, &mut response); }

        Self::discard((*request).into_body(), flight.route.plan.max_body_bytes);
        Identity::echo(response.headers_mut(), snapshot, &flight.id);

        let elapsed_ms = Clock::elapsed_us(flight.started) / 1_000;

        if let Some(actor) = flight.actor() { actor.finish(status, elapsed_ms, false); }

        if flight.observe { self.stats.finish(Outcome::of(status), elapsed_ms, flight.declared, response.body().size_hint().exact().unwrap_or(0)); }

        let Some(mut ticket) = flight.ticket.take() else { return response; };

        if let ( Some(mut entry), Some(log) ) = ( flight.ledger.take(), &self.access ) {

            let counter = Tap::new(0);

            response.body_mut().probe(counter.clone());
            entry.status = status;
            entry.sent = Some(counter);
            ticket.access = Some(( log.clone(), entry ));

        }

        Self::attach(&mut response, ticket);

        response

    }

    pub(super) fn turn ( &self, mut flight: Flight<'_>, request: Box<Req<Body>>, status: u16 ) -> Res<Body> {

        let trace = flight.traced();

        self.reject(request, status, flight.route.plan.max_body_bytes, flight.observe, flight.started, trace)

    }

    pub(super) fn refuse ( &self, status: u16, observe: bool, started: Instant, trace: Option<Trace> ) -> Res<Body> {

        if observe { self.stats.finish(if status >= 500 { Outcome::Failed } else { Outcome::Blocked }, Clock::elapsed_ms(started), 0, 0); }

        if let Some(trace) = trace { trace.finish("rejected", json!({ "status": status })); }

        Response::status(status)

    }

    pub(super) fn reject ( &self, request: Box<Req<Body>>, status: u16, cap: usize, observe: bool, started: Instant, trace: Option<Trace> ) -> Res<Body> {

        Self::discard((*request).into_body(), cap);

        if observe { self.stats.finish(match status { 500.. => Outcome::Failed, 300..=399 => Outcome::Completed, _ => Outcome::Blocked }, Clock::elapsed_ms(started), 0, 0); }

        if let Some(trace) = trace { trace.finish(match status { 403 => "blocked", 300..=399 => "redirected", _ => "rejected" }, json!({ "status": status })); }

        Response::status(status)

    }

    fn charset ( response: &mut Res<Body>, charset: &str ) {

        let Some(kind) = response.headers().get(CONTENT_TYPE).and_then(|value| value.to_str().ok()) else { return; };
        let textual = kind.starts_with("text/") || ["application/json", "application/javascript", "application/xml", "image/svg+xml"].iter().any(|known| kind.starts_with(known));

        if textual && !kind.to_ascii_lowercase().contains("charset=") && let Ok(value) = HeaderValue::from_str(&format!("{kind}; charset={charset}")) { response.headers_mut().insert(CONTENT_TYPE, value); }

    }

    fn decorate ( response: &mut Res<Body>, plan: &Plan, context: &Context, request: &Req<Body>, id: &HeaderValue, snapshot: &Snapshot ) {

        Header::apply(response.headers_mut(), &plan.response_headers);

        if plan.dynamic_response() {

            let vars = Vars { host: request.headers().get(HOST).cloned(), uri: request.uri().clone(), derived: if snapshot.catalog.is_empty() { Vec::new() } else { snapshot.catalog.capture(Self::hint(snapshot, context, request)) } };

            for ( name, value ) in Self::render(&plan.response_headers, context, &vars, id, snapshot, None) { response.headers_mut().insert(name, value); }

        }

    }

    pub(super) fn render ( templates: &[( HeaderName, Rendered )], context: &Context, vars: &Vars, id: &HeaderValue, snapshot: &Snapshot, backend: Option<&Address> ) -> Vec<( HeaderName, HeaderValue )> {

        let peer = &context.peer;
        let client = context.client.borrow();

        let mut out = Vec::new();

        for ( name, template ) in templates {

            let Rendered::Dynamic(template) = template else { continue; };

            let value = template.render(|var, buf| match var {
                Var::RemoteAddr => { let _ = write!(buf, "{}", peer.addr.ip()); }
                Var::RemotePort => { let _ = write!(buf, "{}", peer.addr.port()); }
                Var::Host => buf.extend_from_slice(vars.host.as_ref().map_or(b"", |host| host.as_bytes())),
                Var::Scheme => buf.extend_from_slice(peer.proto.as_bytes()),
                Var::RequestId => buf.extend_from_slice(id.as_bytes()),
                Var::RequestUri => buf.extend_from_slice(vars.uri.path_and_query().map_or("/", |target| target.as_str()).as_bytes()),
                Var::Uri => buf.extend_from_slice(vars.uri.path().as_bytes()),
                Var::Args => buf.extend_from_slice(vars.uri.query().unwrap_or("").as_bytes()),
                Var::UpstreamAddr => { if let Some(addr) = backend { let _ = write!(buf, "{addr}"); } }
                Var::ServerPort => { let _ = write!(buf, "{}", snapshot.config.listen.port()); }
                Var::Msec => { let now = Clock::now_ms(); let _ = write!(buf, "{}.{:03}", now / 1_000, now % 1_000); }
                Var::SslClientVerify => buf.extend_from_slice(if client.is_some() { b"SUCCESS" } else { b"NONE" }),
                Var::SslClientSDn => { if let Some(cert) = client.as_deref() { buf.extend_from_slice(cert.subject.as_bytes()); } }
                Var::SslClientIDn => { if let Some(cert) = client.as_deref() { buf.extend_from_slice(cert.issuer.as_bytes()); } }
                Var::SslClientSerial => { if let Some(cert) = client.as_deref() { buf.extend_from_slice(cert.serial.as_bytes()); } }
                Var::SslClientFingerprint => { if let Some(cert) = client.as_deref() { buf.extend_from_slice(cert.fingerprint.as_bytes()); } }
                Var::Derived(index) => buf.extend_from_slice(vars.derived.get(usize::from(index)).map_or(b"".as_slice(), |value| value)),
            });

            if let Some(value) = value { out.push(( name.clone(), value )); }

        }

        out

    }

    fn attach ( response: &mut Res<Body>, ticket: Ticket ) {

        if response.body().keeps() { response.body_mut().guard(std::rc::Rc::new(ticket)); }

    }

}
