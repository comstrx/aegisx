use std::borrow::Cow;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Instant;

use http::header::{CONNECTION, CONTENT_LENGTH, ETAG, HOST, HeaderMap, HeaderName, HeaderValue, IF_MODIFIED_SINCE, IF_NONE_MATCH, LAST_MODIFIED, RANGE, UPGRADE};
use http_body::Body as _;
use serde_json::json;
use tokio::sync::Semaphore;

use crate::app::{Actor, Backend, Decisions, Entry, Facts, Hint, History, Identity, Key, Lookup, Memory, PoolState, RouteState, Runtime, Snapshot, Store, Trace};
use crate::core::rt::Rt;
use crate::core::time::Clock;
use crate::http::body::{Body, Tap};
use crate::http::encode::Compression;
use crate::http::io::Io;
use crate::http::request::{Req, Request};
use crate::http::response::Res;
use crate::http::rewrite::{Outcome as Rewritten, Rewrite};
use super::arch::{Context, Flight, Handler, Served, Ticket, Vars};

const TRACEPARENT: HeaderName = HeaderName::from_static("traceparent");

impl Handler {

    pub(super) fn open <'a> ( &self, runtime: &'a Runtime, request: &Req<Body>, context: &'a Rc<Context>, ledger: &'a mut Option<Box<Entry>>, served: &'a mut Served, started: Instant ) -> Result<Flight<'a>, u16> {

        let snapshot = &runtime.snapshot;
        let observe = snapshot.config.telemetry.enabled;
        let path = Request::canonical(request.uri().path()).ok_or(400u16)?;
        let host = request.headers().get(HOST).and_then(|value| value.to_str().ok()).unwrap_or("");
        let route: &RouteState = snapshot.locate(host, &path, request.method(), Self::hint(snapshot, context, request)).ok_or(404u16)?;

        if route.spec.internal { return Err(404u16); }

        served.route = Some(route.index);

        let id = match route.policy.tagged || ledger.is_some() {
            true => self.rng.with_mut(|rng| self.arena.with_mut(|arena| Identity::claim(request.headers(), snapshot, &context.peer, rng, arena))),
            false => HeaderValue::from_static("-"),
        };

        if let Some(entry) = ledger.as_mut() { entry.route = Some(route.name.clone()); entry.identify(id.as_bytes()); }

        let mut flight = Flight { runtime, route, context, ledger, served, started, id, ticket: None, rare: None, declared: 0, encoding: None, observe, gunzip: false };

        if route.spec.log == Some(false) { *flight.ledger = None; }

        if let Cow::Owned(canonical) = path { flight.rare().canonical = Some(canonical); }

        if snapshot.actors {

            let key = Self::actor(&self.memory, context, snapshot, request.headers());
            let ( actor, history ) = self.memory.observe(key, runtime.pools.since(started));

            flight.rare().watched = Some(( key, actor, history ));

        }

        if observe && route.policy.capture {

            let actor = flight.watched().map_or_else(String::new, |( key, _, _ )| Memory::hex(key));

            let trace = Trace::begin(&self.capture, flight.id.to_str().unwrap_or(""), &route.name, actor.clone(), started, json!({
                "route"  : route.name,
                "method" : request.method().as_str(),
                "path"   : request.uri().path(),
                "peer"   : context.peer.addr.to_string(),
                "actor"  : actor,
            }));

            flight.rare().trace = Some(trace);

        }

        Ok(flight)

    }

    pub(super) fn guard ( &self, flight: &mut Flight<'_>, request: &Req<Body> ) -> Option<( u16, u64 )> {

        let route = flight.route;
        let started = flight.started;

        let admitted = route.fence.as_ref().is_none_or(|fence| fence.admits(Identity::client(request.headers(), &flight.runtime.snapshot, &flight.context.peer)));
        let guarded = route.auth.is_some() || route.bearer.is_some() || route.verify.is_some() || route.link.is_some();

        if route.spec.satisfy_any && admitted && route.fence.is_some() { flight.rare().satisfied = true; }

        if route.spec.deny || !admitted && !(route.spec.satisfy_any && guarded) {

            if let Some(actor) = flight.actor() { actor.finish(403, Clock::elapsed_ms(started), true); }

            return Some(( 403, 0 ));

        }

        let keyed = route.keyed.as_ref().and_then(|spec| spec.material(Hint { ip: flight.context.peer.addr.ip(), secure: flight.context.peer.proto.as_bytes() == b"https", headers: request.headers(), uri: request.uri() }, |bytes| self.memory.handle(b"rate", bytes)));
        let rare = flight.rare.as_deref_mut()?;
        let ( key, actor, history ) = rare.watched.as_ref()?;

        if let Some(decisions) = &self.decisions && route.policy.decisions && let Some(verdict) = decisions.lookup(&Decisions::key(&route.name, key)) {

            actor.finish(403, Clock::elapsed_ms(started), true);

            if let Some(trace) = &mut rare.trace { trace.record("decision", json!({ "reason": verdict.reason, "source": verdict.source, "key": verdict.key, "expires_ms": verdict.expires_ms })); }

            return Some(( 403, 0 ));

        }

        if route.policy.rate_limit > 0 {

            let prior = match ( keyed, route.policy.scoped ) {
                ( Some(subject), scoped ) => self.memory.charge(subject, if scoped { route.index as u32 } else { u32::MAX }, flight.runtime.pools.since(started)),
                ( None, true ) => self.memory.charge(*key, route.index as u32, flight.runtime.pools.since(started)),
                ( None, false ) => history.requests,
            };

            if prior >= route.policy.rate_limit {

                actor.finish(429, Clock::elapsed_ms(started), true);

                return Some(( 429, 0 ));

            }

        }

        if route.policy.pace > 0 {

            let now_us = started.saturating_duration_since(flight.runtime.pools.started).as_micros() as u64;

            let wait = match ( keyed, route.policy.paced ) {
                ( Some(subject), paced ) => self.memory.pace(subject, if paced { route.index as u32 } else { u32::MAX }, now_us, route.policy.pace, route.policy.burst),
                ( None, true ) => self.memory.pace(*key, route.index as u32, now_us, route.policy.pace, route.policy.burst),
                ( None, false ) => actor.pace(now_us, route.policy.pace, route.policy.burst),
            };

            if let Some(wait_us) = wait {

                actor.finish(429, Clock::elapsed_ms(started), true);

                return Some(( 429, wait_us.div_ceil(1_000_000).max(1) ));

            }

        }

        if !route.rules.is_empty() {

            let now_us = started.saturating_duration_since(flight.runtime.pools.started).as_micros() as u64;
            let hint = Hint { ip: flight.context.peer.addr.ip(), secure: flight.context.peer.proto.as_bytes() == b"https", headers: request.headers(), uri: request.uri() };
            let subjects = route.rules.iter().filter_map(|rule| rule.key.material(hint, |bytes| self.memory.handle(b"rule", bytes)).map(|subject| ( subject, rule.scope, rule.rate, rule.burst )));

            if let Some(wait_us) = self.memory.pace_all(subjects, now_us) {

                actor.finish(429, Clock::elapsed_ms(started), true);

                return Some(( 429, wait_us.div_ceil(1_000_000).max(1) ));

            }

        }

        if route.policy.concurrency > 0 {

            let gauge = keyed.map(|subject| self.memory.observe(subject, flight.runtime.pools.since(started)));
            let in_flight = gauge.as_ref().map_or(history.in_flight, |( _, held )| held.in_flight);

            if in_flight >= route.policy.concurrency {

                actor.finish(503, Clock::elapsed_ms(started), true);

                if let Some(trace) = &mut rare.trace { trace.record("concurrency", json!({ "in_flight": in_flight, "limit": route.policy.concurrency })); }

                return Some(( 503, 0 ));

            }

            rare.gauge = gauge.map(|( held, _ )| held);

        }

        None

    }

    pub(super) fn shape ( &self, flight: &mut Flight<'_>, request: &Req<Body> ) -> Option<( u16, Option<HeaderValue> )> {

        let route = flight.route;
        let snapshot = &flight.runtime.snapshot;

        match Rewrite::run(&route.rewrites, flight.path(request), request.uri().query()) {
            Rewritten::Keep => {}
            Rewritten::Target(target) => flight.rare().rewritten = Some(target),
            Rewritten::Redirect(location, code) => return Some(( code.as_u16(), HeaderValue::from_str(&location).ok() )),
        }

        flight.declared = if request.body().is_end_stream() { 0 } else { Self::length(request.headers()) };
        flight.encoding = if route.plan.compress { snapshot.compression.pick(request.headers()) } else { None };

        if let Some(entry) = flight.ledger.as_mut() { entry.received = flight.declared; }

        if flight.declared > route.plan.max_body_bytes as u64 { return Some(( 413, None )); }

        let gauge = flight.rare.as_deref().and_then(|rare| rare.gauge.clone());
        let Some(ticket) = self.admit(flight.context, snapshot.quota, flight.observe, gauge.as_ref().or(flight.actor())) else { return Some(( 503, None )); };

        flight.ticket = Some(ticket);

        None

    }

    pub(super) fn cached ( &self, flight: &mut Flight<'_>, request: &Req<Body> ) -> Option<Res<Body>> {

        let store = self.cache.as_ref().filter(|_| Store::cacheable(request.method(), request.headers()))?;
        let mut key = store.key(request.headers().get(HOST).and_then(|value| value.to_str().ok()).unwrap_or(""), request.uri(), request.headers());
        let now_ms = Clock::wall_ms(flight.started);

        let ranged = request.headers().contains_key(RANGE);

        match store.lookup(&mut key, request.headers(), now_ms) {
            Lookup::Miss | Lookup::Pass | Lookup::Revalidate(_) | Lookup::Refresh(_) if ranged => None,
            Lookup::Miss => { flight.rare().filling = Some(key); None }
            Lookup::Pass => {

                let rare = flight.rare();

                rare.filling = Some(key);
                rare.passing = true;

                None

            }
            Lookup::Revalidate(entry) => {

                let rare = flight.rare();

                rare.filling = Some(key);
                rare.fallback = store.rescue.then(|| entry.clone());
                rare.revalidating = Some(entry);

                None

            }
            Lookup::Refresh(entry) => {

                let rare = flight.rare();

                rare.filling = Some(key);
                rare.fallback = store.rescue.then_some(entry);

                None

            }
            Lookup::Hit(entry) | Lookup::Stale(entry) => {

                let stale = now_ms >= entry.expires_ms;
                let response = Store::respond(&entry, request.method(), request.headers(), now_ms, stale);

                if let Some(trace) = flight.traced() { trace.finish("cached", json!({ "status": response.status().as_u16(), "stale": stale })); }

                flight.served.proxied = true;

                Some(response)

            }
        }

    }

    pub(super) fn hint <'r> ( snapshot: &Snapshot, context: &Context, request: &'r Req<Body> ) -> Hint<'r> {

        let ip = if snapshot.catalog.located { Identity::client(request.headers(), snapshot, &context.peer) } else { context.peer.addr.ip() };

        Hint { ip, secure: context.peer.proto.as_bytes() == b"https", headers: request.headers(), uri: request.uri() }

    }

    pub(super) fn contend ( &self, flight: &mut Flight<'_> ) -> Option<Arc<Semaphore>> {

        let store = self.cache.as_ref().filter(|store| store.lock_ms > 0)?;
        let rare = flight.rare.as_deref_mut().filter(|rare| !rare.passing && rare.claim.is_none() && rare.revalidating.is_none() && rare.fallback.is_none())?;

        match store.claim(rare.filling.as_ref()?) {
            Ok(claim) => { rare.claim = Some(claim); None }
            Err(gate) => Some(gate),
        }

    }

    pub(super) fn aim ( &self, flight: &mut Flight<'_>, request: &mut Req<Body> ) {

        let route = flight.route;
        let snapshot = &flight.runtime.snapshot;
        let context = flight.context;

        flight.gunzip = route.plan.gunzip && !Compression::accepts_gzip(request.headers());

        if let Some(analyser) = &self.analyser && flight.rare.as_deref().is_some_and(|rare| rare.trace.is_some()) {

            let ( scan, response_scan ) = analyser.scan_bytes();
            let filling = flight.rare.as_deref().is_some_and(|rare| rare.filling.is_some());
            let response_scan = if filling { response_scan.max(self.cache.as_ref().map_or(0, |store| store.max_object)) } else { response_scan };
            let facts = Facts::of(request.method(), request.uri(), request.headers(), flight.declared);
            let history = flight.watched().map_or_else(History::default, |( _, _, history )| *history);

            flight.rare().probe = Some(Box::new(( facts.admission(history), Facts::sample(request.method(), request.uri(), scan), Tap::new(response_scan) )));

        }

        if let Some(upgrade) = request.headers().get(UPGRADE).cloned().filter(|_| request.headers().get_all(CONNECTION).iter().any(|value| value.as_bytes().split(|byte| *byte == b',').any(|token| token.trim_ascii().eq_ignore_ascii_case(b"upgrade")))) {

            let upgrading = Io::upgrading(request.extensions_mut());
            let rare = flight.rare();

            rare.upgrade = Some(upgrade);
            rare.upgrading = upgrading;

        }

        if let Some(chain) = Identity::chain(request.headers(), snapshot, &context.peer) { flight.rare().chain = Some(chain); }

        if snapshot.config.identity.traceparent && !request.headers().contains_key(&TRACEPARENT) && let Some(trace) = self.rng.with_mut(Identity::trace) { flight.rare().rendered.push(( TRACEPARENT, trace )); }

        let dynamic = route.plan.dynamic_request();

        if dynamic || route.plan.dynamic_response() {

            let vars = Box::new(Vars { host: request.headers().get(HOST).cloned(), uri: request.uri().clone(), derived: if snapshot.catalog.is_empty() { Vec::new() } else { snapshot.catalog.capture(Self::hint(snapshot, context, request)) } });

            let rendered = if dynamic { Self::render(&route.plan.request_headers, context, &vars, &flight.id, snapshot, None) } else { Vec::new() };
            let rare = flight.rare();

            let granted = std::mem::replace(&mut rare.rendered, rendered);

            rare.rendered.extend(granted);
            rare.vars = Some(vars);

        }

        if let Some(rare) = flight.rare.as_deref_mut() && let Some(entry) = &rare.revalidating {

            if let Some(etag) = entry.headers.get(ETAG) { rare.rendered.push(( IF_NONE_MATCH, etag.clone() )); }

            if let Some(modified) = entry.headers.get(LAST_MODIFIED) { rare.rendered.push(( IF_MODIFIED_SINCE, modified.clone() )); }

        }

        let body = std::mem::replace(request.body_mut(), Body::Empty);

        if !body.is_end_stream() { *request.body_mut() = Body::limited(body, match route.plan.max_body_bytes { 0 => usize::MAX, cap => cap }, route.plan.client_timeout_ms); }

        if let Some(probing) = flight.rare.as_deref().and_then(|rare| rare.probe.as_ref()) { request.body_mut().probe(probing.1.clone()); }

    }

    pub(super) fn choose <'p> ( &self, flight: &mut Flight<'_>, pool: &'p PoolState, pending: &Req<Body>, excluded: &[usize], now: u64, attempt: usize ) -> Option<( &'p Arc<Backend>, Option<usize> )> {

        let hint = Hint { ip: flight.context.peer.addr.ip(), secure: flight.context.peer.proto.as_bytes() == b"https", headers: pending.headers(), uri: pending.uri() };
        let stuck = pool.sticky.as_ref().and_then(|sticky| sticky.wanted(pending.headers(), &pool.backends));
        let backend = self.picker.with_mut(|picker| picker.pick(&flight.runtime.pools, pool, excluded, now, hint)).and_then(|index| pool.backends.get(index))?;

        if let Some(trace) = flight.trace() { trace.record("forwarded", json!({ "backend": backend.addr.to_string(), "attempt": attempt + 1 })); }

        Some(( backend, stuck ))

    }

    pub(super) fn admit ( &self, context: &Rc<Context>, quota: usize, observe: bool, actor: Option<&Arc<Actor>> ) -> Option<Ticket> {

        if self.stats.inflight.get() >= quota as u64 { return None; }

        self.stats.inflight.add(1);
        context.inflight.set(context.inflight.get() + 1);

        if observe { self.stats.active.add(1); }

        if let Some(actor) = actor { actor.hold(); }

        Some(Ticket { context: context.clone(), counted: observe, lease: None, actor: actor.cloned(), pending: None, access: None, fill: None })

    }

    pub(super) fn actor ( memory: &Memory, context: &Context, snapshot: &Snapshot, headers: &HeaderMap ) -> Key {

        let header = snapshot.names.actor.as_ref().filter(|_| context.peer.trusted).and_then(|name| headers.get(name)).and_then(|value| HeaderValue::from_bytes(value.as_bytes()).ok());
        let mut cache = context.actor.borrow_mut();

        if let Some(( cached, key )) = cache.as_ref() && *cached == header { return *key; }

        let key = match &header {
            Some(value) => memory.handle(b"actor", value.as_bytes()),
            None => memory.handle(b"peer", context.peer.addr.ip().to_string().as_bytes()),
        };

        *cache = Some(( header, key ));

        key

    }

    pub(super) fn discard ( body: Body, cap: usize ) {

        if body.is_end_stream() { return; }

        Rt::spawn_local(Body::drain(body, cap.max(65_536)));

    }

    pub(super) fn length ( headers: &HeaderMap ) -> u64 {

        headers.get(CONTENT_LENGTH).and_then(|value| value.to_str().ok()).and_then(|value| value.parse::<u64>().ok()).unwrap_or(0)

    }

}
