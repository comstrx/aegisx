use std::cell::{Cell, RefCell};
use std::net::SocketAddr;
use std::rc::Rc;
use std::time::Instant;

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use http::{Method, StatusCode};
use http::header::{ALLOW, ALT_SVC, CONNECTION, CONTENT_LENGTH, CONTENT_TYPE, HOST, HeaderMap, HeaderName, HeaderValue, LOCATION, RETRY_AFTER, SEC_WEBSOCKET_ACCEPT, SEC_WEBSOCKET_KEY, SEC_WEBSOCKET_VERSION, UPGRADE, WWW_AUTHENTICATE};
use http_body::Body as _;
use serde_json::json;

use crate::app::{Bearer, Entry, Hint, Identity, Lease, Outcome, Page, Picker, Runtime, Snapshot, State, Verifier};
use crate::core::arena::Arena;
use crate::core::log::debug;
use crate::core::rand::Rng;
use crate::core::sync::Local;
use crate::core::time::Clock;
use crate::http::body::Body;
use crate::http::fastcgi::{Call, Fcgi};
use crate::http::files::Fetch;
use crate::http::proxy::{Forward, Proxy, Target};
use crate::http::request::{Req, Request};
use crate::http::response::{Abort, Res, Response};
use crate::http::upstream::{Client, Replay};
use super::arch::{Context, Flow, Handler, Hop, Served, Sinks};

const FORWARDED_METHOD: HeaderName = HeaderName::from_static("x-forwarded-method");
const FORWARDED_URI: HeaderName = HeaderName::from_static("x-forwarded-uri");
const FORWARDED_HOST: HeaderName = HeaderName::from_static("x-forwarded-host");
const ACCEL_REDIRECT: HeaderName = HeaderName::from_static("x-accel-redirect");
const REPLAY_BYTES: u64 = 65_536;

impl Handler {

    pub fn new ( state: State, client: Client, rng: Rng, sinks: Sinks, worker: usize ) -> Self {

        let runtime = state.lens();
        let memory = state.memory();
        let decisions = state.decisions();
        let files = state.files();
        let cache = state.cache();
        let picker = Picker::new(&runtime.get().pools);
        let scripts = Fcgi::new(runtime.get().snapshot.config.client_settings());

        let Sinks { stats, capture, analyser, access } = sinks;

        Self { runtime, client, scripts, hooks: Local::new(( 0, None )), picker: Local::new(picker), rng: Local::new(rng), arena: Local::new(Arena::new()), stats, capture, analyser, memory, decisions, access, files, cache, worker, tunnels: Rc::new(Cell::new(0)) }

    }

    pub fn connect ( &self, addr: SocketAddr, secure: bool ) -> Context {

        let runtime = self.runtime.get();

        Context { peer: Identity::peer(&runtime.snapshot, addr, secure), stats: self.stats.clone(), client: RefCell::new(None), actor: RefCell::new(None), seen: Cell::new(Instant::now()), inflight: Cell::new(0), served: Cell::new(false), count: Cell::new(0), born: Instant::now() }

    }

    pub async fn handle ( &self, mut request: Box<Req<Body>>, context: Rc<Context> ) -> Res<Body> {

        let runtime = self.runtime.get();

        if request.version() >= http::Version::HTTP_2 && let Some(host) = request.uri().authority().and_then(|authority| HeaderValue::from_str(authority.as_str()).ok()) { request.headers_mut().insert(HOST, host); }

        let bridged = request.version() == http::Version::HTTP_2 && request.method() == Method::CONNECT && request.extensions().get::<hyper::ext::Protocol>().is_some_and(|protocol| protocol.as_str().eq_ignore_ascii_case("websocket"));

        if bridged {

            let nonce = self.rng.with_mut(|rng| rng.next_u128()).to_le_bytes();

            *request.method_mut() = Method::GET;
            request.headers_mut().insert(CONNECTION, HeaderValue::from_static("upgrade"));
            request.headers_mut().insert(UPGRADE, HeaderValue::from_static("websocket"));
            request.headers_mut().insert(SEC_WEBSOCKET_VERSION, HeaderValue::from_static("13"));

            if let Ok(key) = HeaderValue::from_str(&STANDARD.encode(nonce)) { request.headers_mut().insert(SEC_WEBSOCKET_KEY, key); }

        }

        let pages = runtime.snapshot.pages.then(|| Box::new(( request.headers().get(HOST).cloned(), request.method() == Method::HEAD )));
        let mut ledger = self.access.as_ref().map(|log| Box::new(log.open(&request, context.peer.addr, context.peer.proto.as_bytes() == b"https", context.client.borrow().as_deref().map(|cert| cert.subject.as_ref()))));
        let mut served = Served::default();
        let lasting = request.version() < http::Version::HTTP_2;
        let mut response = self.serve(&runtime, request, &context, &mut ledger, &mut served).await;

        if bridged {

            match response.status() {
                StatusCode::SWITCHING_PROTOCOLS => {

                    *response.status_mut() = StatusCode::OK;

                    for name in [CONNECTION, UPGRADE, SEC_WEBSOCKET_ACCEPT] { response.headers_mut().remove(name); }

                }
                status if status.is_success() => {

                    *response.status_mut() = StatusCode::BAD_GATEWAY;
                    *response.body_mut() = Body::Empty;
                    response.headers_mut().remove(CONTENT_LENGTH);

                }
                _ => {}
            }

        }

        if lasting && context.spent(&runtime.snapshot.config.server) { response.headers_mut().insert(CONNECTION, HeaderValue::from_static("close")); }

        match pages {
            Some(pages) => Box::pin(self.paged(&runtime.snapshot, response, pages.0, pages.1, ledger, served)).await,
            None => self.seal(&runtime.snapshot, response, ledger, served),
        }

    }

    async fn paged ( &self, snapshot: &Snapshot, mut response: Res<Body>, host: Option<HeaderValue>, head: bool, ledger: Option<Box<Entry>>, served: Served ) -> Res<Body> {

        if let Some(page) = Self::page(snapshot, &served, response.status().as_u16()) { self.substitute(snapshot, page, host.as_ref(), head, &mut response).await; }

        self.seal(snapshot, response, ledger, served)

    }

    fn seal ( &self, snapshot: &Snapshot, mut response: Res<Body>, ledger: Option<Box<Entry>>, served: Served ) -> Res<Body> {

        if snapshot.paced && let Some(route) = served.route.and_then(|index| snapshot.routes.get(index)) && route.plan.bandwidth > 0 && !response.body().is_end_stream() {

            let inner = std::mem::replace(response.body_mut(), Body::Empty);

            *response.body_mut() = Body::paced(inner, route.plan.bandwidth, route.plan.bandwidth_after);

        }

        if let ( Some(mut entry), Some(log) ) = ( ledger, &self.access ) { entry.status = response.status().as_u16(); log.record(&entry); }

        if let Some(alt) = &snapshot.alt_svc { response.headers_mut().insert(ALT_SVC, alt.clone()); }

        response

    }

    fn page <'s> ( snapshot: &'s Snapshot, served: &Served, status: u16 ) -> Option<&'s Page> {

        let route = served.route.and_then(|index| snapshot.routes.get(index));

        if served.proxied && !route.is_some_and(|route| route.spec.intercept_errors) { return None; }

        route.and_then(|route| route.errors.as_ref()).unwrap_or(&snapshot.errors).find(status)

    }

    async fn substitute ( &self, snapshot: &Snapshot, page: &Page, host: Option<&HeaderValue>, head: bool, response: &mut Res<Body> ) {

        let status = response.status().as_u16();

        let mut replacement = match &page.redirect {
            Some(location) => {

                let mut redirect = Response::status(page.code.filter(|code| (300..=399).contains(code)).unwrap_or(302));

                redirect.headers_mut().insert(LOCATION, location.clone());

                redirect

            }
            None => {

                let host = host.and_then(|value| value.to_str().ok()).unwrap_or("");
                let Some(route) = snapshot.route(host, &page.path, &Method::GET, &HeaderMap::new()) else { return; };
                let Some(files) = &route.files else { return; };
                let method = if head { Method::HEAD } else { Method::GET };
                let fetched = Box::pin(files.serve(&self.files, Fetch { method: &method, headers: &HeaderMap::new(), path: &page.path, strip: route.plan.mount.len(), query: None, now_ms: Clock::wall_ms(Instant::now()) })).await;

                if fetched.status() != http::StatusCode::OK { return; }

                fetched

            }
        };

        if page.redirect.is_none() && let Ok(code) = http::StatusCode::from_u16(page.code.unwrap_or(status)) { *replacement.status_mut() = code; }

        for name in [&snapshot.names.request_id, &RETRY_AFTER, &WWW_AUTHENTICATE, &ALLOW] {

            if let Some(value) = response.headers().get(name) { replacement.headers_mut().insert(name.clone(), value.clone()); }

        }

        *response = replacement;

    }

    async fn serve ( &self, runtime: &Runtime, mut request: Box<Req<Body>>, context: &Rc<Context>, ledger: &mut Option<Box<Entry>>, served: &mut Served ) -> Res<Body> {

        let started = Instant::now();
        let observe = runtime.snapshot.config.telemetry.enabled;

        context.seen.set(started);

        if observe { self.stats.total.add(1); }

        let mut flight = match self.open(runtime, &request, context, ledger, served, started) {
            Ok(flight) => flight,
            Err(status) => return self.reject(request, status, runtime.snapshot.config.limits.max_body_bytes, observe, started, None),
        };

        let route = flight.route;

        if let Some(( status, after )) = self.guard(&mut flight, &request) {

            let mut response = self.turn(flight, request, status);

            if after > 0 { response.headers_mut().insert(RETRY_AFTER, HeaderValue::from(after)); }

            return response;

        }

        let satisfied = flight.rare.as_deref().is_some_and(|rare| rare.satisfied);

        if !satisfied && let Some(link) = &route.link && let Err(status) = link.verify(request.uri(), Clock::now_ms() / 1_000) { return self.turn(flight, request, status); }

        if !satisfied && let Some(auth) = &route.auth && !Box::pin(auth.allows(request.headers())).await {

            if let Some(actor) = flight.actor() { actor.finish(401, Clock::elapsed_ms(started), true); }

            if let Some(trace) = flight.trace() { trace.record("unauthorized", json!({ "scheme": "basic" })); }

            let mut response = self.turn(flight, request, 401);

            response.headers_mut().insert(WWW_AUTHENTICATE, auth.challenge());

            return response;

        }

        if let Some(bearer) = route.bearer.as_ref().filter(|_| !satisfied) {

            match bearer.admit(request.headers(), Clock::now_ms()) {
                Ok(grant) => {

                    for name in bearer.names() { request.headers_mut().remove(name); }

                    if !grant.is_empty() { flight.rare().rendered.extend(grant.iter().cloned()); }

                }
                Err(fault) => {

                    if let Some(actor) = flight.actor() { actor.finish(401, Clock::elapsed_ms(started), true); }

                    if let Some(trace) = flight.trace() { trace.record("unauthorized", json!({ "scheme": "bearer", "reason": fault.reason() })); }

                    let mut response = self.turn(flight, request, 401);

                    response.headers_mut().insert(WWW_AUTHENTICATE, Bearer::challenge(fault));

                    return response;

                }
            }

        }

        if let Some(verifier) = route.verify.as_ref().filter(|_| !satisfied) {

            match Box::pin(self.verify(verifier, runtime, context, &request, &flight.id, started)).await {
                Ok(headers) => {

                    for name in &verifier.copy { request.headers_mut().remove(name); }

                    if !headers.is_empty() { flight.rare().rendered.extend(headers); }

                }
                Err(response) => {

                    let mut response = *response;
                    let status = response.status().as_u16();

                    if let Some(actor) = flight.actor() { actor.finish(status, Clock::elapsed_ms(started), true); }

                    if let Some(trace) = flight.trace() { trace.record("forward_auth", json!({ "status": status })); }

                    Self::discard((*request).into_body(), route.plan.max_body_bytes);

                    if observe { self.stats.finish(if status >= 500 { Outcome::Failed } else { Outcome::Blocked }, Clock::elapsed_ms(started), 0, 0); }

                    if let Some(trace) = flight.traced() { trace.finish("rejected", json!({ "status": status })); }

                    Identity::echo(response.headers_mut(), &runtime.snapshot, &flight.id);

                    return response;

                }
            }

        }

        if let Some(( status, location )) = self.shape(&mut flight, &request) {

            let mut response = self.turn(flight, request, status);

            if let Some(location) = location { response.headers_mut().insert(LOCATION, location); }

            return response;

        }

        if route.spec.abort {

            let mut response = Res::new(Body::Empty);

            response.extensions_mut().insert(Abort);

            return self.deliver(response, request, flight);

        }

        if let Some(reply) = &route.reply {

            if let Some(trace) = flight.traced() { trace.finish("served", json!({ "status": reply.status.as_u16() })); }

            return self.deliver(reply.response(), request, flight);

        }

        if let Some(method) = &route.method { *request.method_mut() = method.clone(); }

        if runtime.snapshot.config.hooks.request || runtime.snapshot.config.hooks.response {

            match self.hooked(&runtime.snapshot, &mut flight, &mut request) {
                Ok(None) => {}
                Ok(Some(response)) => return self.deliver(response, request, flight),
                Err(status) => return self.turn(flight, request, status),
            }

        }

        if let Some(files) = &route.files && route.script.as_ref().is_none_or(|script| matches!(*request.method(), Method::GET | Method::HEAD) && !script.covers(flight.path(&request))) {

            let fetched = {

                let rewritten = flight.rare.as_deref().and_then(|rare| rare.rewritten.as_deref());
                let target = rewritten.map_or(flight.path(&request), |target| target.split('?').next().unwrap_or(target));
                let strip = if rewritten.is_some() { 0 } else { route.plan.mount.len() };
                let fetch = Fetch { method: request.method(), headers: request.headers(), path: target, strip, query: request.uri().query(), now_ms: Clock::wall_ms(started) };

                match route.tries.is_empty() { true => Some(Box::pin(files.serve(&self.files, fetch)).await), false => Box::pin(files.attempt(&self.files, fetch, &route.tries)).await }

            };

            if let Some(response) = fetched {

                if let Some(trace) = flight.traced() { trace.finish("served", json!({ "status": response.status().as_u16() })); }

                return self.deliver(response, request, flight);

            }

        }

        let Some(pool) = route.pool.and_then(|index| runtime.pools.get(index)) else { return self.turn(flight, request, 503); };

        if route.plan.cache && !route.bypass.iter().any(|index| { let value = runtime.snapshot.catalog.value(*index, Self::hint(&runtime.snapshot, flight.context, &request)); !value.is_empty() && *value != *b"0" }) {

            if let Some(response) = self.cached(&mut flight, &request) { return self.deliver(response, request, flight); }

            for _ in 0..2 {

                let Some(( store, key )) = self.cache.as_ref().zip(flight.rare.as_deref().and_then(|rare| rare.filling.clone())) else { break; };
                let now_ms = Clock::wall_ms(started);

                if !store.shelved(&key, now_ms) || !Box::pin(store.thaw(key, now_ms)).await { break; }

                if let Some(response) = self.cached(&mut flight, &request) { return self.deliver(response, request, flight); }

            }

            if let Some(gate) = self.contend(&mut flight) && let Some(store) = &self.cache {

                Box::pin(store.wait(gate)).await;

                if let Some(response) = self.cached(&mut flight, &request) { return self.deliver(response, request, flight); }

            }

        }

        self.aim(&mut flight, &mut request);

        let spare = pool.attempts > 1 && pool.backends.len() > 1;

        let small = spare && flight.declared > 0 && flight.declared <= REPLAY_BYTES;

        if (route.plan.buffer_request || small) && !request.body().is_end_stream() {

            let limits = &runtime.snapshot.config.limits;

            let held = match small {
                true => Box::pin(request.body_mut().gather()).await,
                false => Box::pin(request.body_mut().spool(limits.spool_bytes, &limits.spool_dir)).await,
            };

            if let Err(error) = held {

                let trace = flight.traced();

                return self.refuse(error.status(), observe, started, trace);

            }

        }

        if let Some(mirror) = route.mirror { self.mirror(&flight, &request, mirror); }

        let head = request.method() == Method::HEAD;
        let idempotent = matches!(*request.method(), Method::GET | Method::HEAD | Method::OPTIONS | Method::PUT | Method::DELETE | Method::TRACE);
        let retryable = pool.attempts > 1 && (idempotent || pool.retry.non_idempotent);
        let replay = Replay { failure: spare && (pool.retry.connect || retryable && pool.retry.replays()), status: retryable && pool.retry.rerun() };
        let mut request = Some(request);
        let mut excluded: Vec<usize> = Vec::new();
        let mut status = 503;

        for attempt in 0..pool.attempts.max(1) {

            let Some(pending) = request.take() else { break; };
            let now = runtime.pools.since(started);
            let more = attempt + 1 < pool.attempts;

            let Some(( backend, stuck )) = self.choose(&mut flight, pool, &pending, &excluded, now, attempt) else {

                Self::discard((*pending).into_body(), route.plan.max_body_bytes);

                break;

            };

            let hop = Hop { pool, backend, lease: pool.counting.then(|| Lease::new(backend.clone())), stuck, attempt, elapsed_us: 0 };
            let outcome = match &route.script {
                Some(script) => Box::pin(self.scripts.exchange(&backend.upstream, Call { script, peer: flight.context.peer.addr, port: runtime.snapshot.config.listen.port(), secure: flight.context.peer.proto.as_bytes() == b"https", timeout_ms: route.plan.timeout_ms, keep: pool.keepalive }, pending)).await.map(|response| ( response, None )),
                None => { let forward = flight.forward(&backend.upstream); Proxy::forward(&self.client, &forward, started, if more { replay } else { Replay::default() }, pending).await }
            };

            let landed = self.land(&mut flight, hop, outcome, Flow { head, retryable, more, now }, &mut excluded);

            match landed {
                Ok(( mut response, hop, false )) => {

                    if runtime.snapshot.internal && let Some(target) = response.headers_mut().remove(&ACCEL_REDIRECT) { return Box::pin(self.relocate(flight, response, hop, target)).await; }

                    return self.complete(flight, response, hop);

                }
                Ok(( response, hop, true )) => return Box::pin(self.buffered(flight, response, hop)).await,
                Err(( again, failed )) => {

                    request = again;

                    if let Some(failed) = failed { status = failed; }

                }
            }

        }

        self.fail(flight, status)

    }

    async fn verify ( &self, verifier: &Verifier, runtime: &Runtime, context: &Rc<Context>, request: &Req<Body>, id: &HeaderValue, started: Instant ) -> Result<Vec<( HeaderName, HeaderValue )>, Box<Res<Body>>> {

        let ( pools, snapshot ) = ( &runtime.pools, &runtime.snapshot );
        let Some(pool) = pools.get(verifier.pool) else { return Err(Box::new(Response::status(502))); };
        let now = pools.since(started);
        let hint = Hint { ip: context.peer.addr.ip(), secure: context.peer.proto.as_bytes() == b"https", headers: request.headers(), uri: request.uri() };
        let Some(backend) = self.picker.with_mut(|picker| picker.pick(pools, pool, &[], now, hint)).and_then(|index| pool.backends.get(index)) else { return Err(Box::new(Response::status(502))); };
        let lease = pool.counting.then(|| Lease::new(backend.clone()));
        let chain = Identity::chain(request.headers(), snapshot, &context.peer);
        let add = Identity::outgoing(snapshot, &context.peer, chain.as_ref(), id);
        let drops = Identity::drops(snapshot, &context.peer);
        let mut extra: Vec<( HeaderName, HeaderValue )> = Vec::with_capacity(3);

        if let Ok(method) = HeaderValue::from_str(request.method().as_str()) { extra.push(( FORWARDED_METHOD, method )); }

        if let Ok(uri) = HeaderValue::from_str(Request::target(request.uri())) { extra.push(( FORWARDED_URI, uri )); }

        if let Some(host) = request.headers().get(HOST) { extra.push(( FORWARDED_HOST, host.clone() )); }

        let mut probe = Req::new(Body::Empty);

        *probe.method_mut() = Method::GET;
        *probe.headers_mut() = request.headers().clone();
        probe.headers_mut().remove(CONTENT_TYPE);

        let forward = Forward { plan: &verifier.plan, upstream: &backend.upstream, target: Target::Rewritten(&verifier.path), add, rendered: &extra, drop: drops, upgrade: None };
        let mut outcome = Proxy::forward(&self.client, &forward, started, Replay::default(), Box::new(probe)).await;

        drop(lease);

        if let Ok(( response, _ )) = &mut outcome { Proxy::settle(response, &verifier.plan, &backend.upstream); }

        match outcome {
            Ok(( response, _ )) if response.status().is_success() => {

                backend.succeed();

                Ok(verifier.copy.iter().filter_map(|name| response.headers().get(name).map(|value| ( name.clone(), value.clone() ))).collect())

            }
            Ok(( response, _ )) => { backend.succeed(); Err(Box::new(response)) }
            Err(failure) => {

                if failure.connect { backend.fail(pool, now); }

                debug!(peer = %context.peer.addr, backend = %backend.addr, error = %failure.error, "forward auth failed");

                Err(Box::new(Response::status(502)))

            }
        }

    }

}
