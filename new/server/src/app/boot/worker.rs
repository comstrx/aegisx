use std::rc::Rc;

use std::net::SocketAddr;

use http::header::{HOST, LOCATION};

use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::app::{Acme, Handler, Log, Pools, Relay, Sinks, Telemetry, Tokens};
use crate::config::AcmeChallenge;
use crate::http::body::Body;
use crate::http::request::Req;
use crate::http::response::{Res, Response};
use crate::http::server::{Accept, Listener};
use crate::core::error::{AppError, AppResult};
use crate::core::log::{debug, warn};
use crate::core::rand::Rng;
use crate::core::rt::Rt;
use crate::core::sync::Swap;
use crate::http::upstream::Client;
use crate::http::h3::Quic;
use crate::http::server::Server;
use super::arch::Worker;

impl Worker {

    pub async fn run ( self ) -> AppResult<()> {

        let listener = self.listeners.lock().ok().and_then(|mut slots| slots.get_mut(self.index).and_then(Option::take))
            .ok_or_else(|| AppError::message(format!("listener {} missing", self.index)))?;

        let ( Some(stats), Some(capture) ) = ( self.telemetry.workers.get(self.index), self.telemetry.captures.get(self.index) ) else {

            return Err(AppError::message(format!("telemetry slot {} missing", self.index)));

        };

        let client = Client::new(self.config.client_settings());
        let access = self.access.as_ref().and_then(|sink| sink.log(&self.config.access));

        if let Some(log) = &access { Log::start(log); }

        let sinks = Sinks { stats: stats.clone(), capture: capture.clone(), analyser: self.analyser.clone(), access };
        let handler = Rc::new(Handler::new(self.state.clone(), client, Rng::seeded()?, sinks, self.index));
        let bridges = handler.clone();

        if self.index == 0 {

            Rt::spawn_local(Pools::probe_loop(self.state.clone(), self.stop.clone()));

            if self.config.telemetry.otlp.is_some() { Rt::spawn_local(Telemetry::export(self.telemetry.clone(), self.state.clone(), self.stop.clone())); }

            if self.config.client.resolve_ms > 0 && !Pools::names(&self.config).is_empty() { Rt::spawn_local(Pools::resolve_loop(self.state.clone(), self.stop.clone(), self.config.client.resolve_ms)); }

            if let Some(acme) = self.config.tls.as_ref().and_then(|tls| tls.acme.clone()) {

                if acme.challenge == AcmeChallenge::Http01 && let Some(tokens) = self.state.tokens() { self.challenge_listener(acme.listen.unwrap_or_else(|| SocketAddr::from(( [0, 0, 0, 0], 80 ))), tokens)?; }

                Rt::spawn_local(Acme::new(acme, self.state.clone()).run(self.stop.clone()));

            }

        }

        for ( position, settings ) in self.config.streams.iter().enumerate().filter(|( _, settings )| settings.udp) {

            let Some(socket) = self.datagrams.lock().ok().and_then(|mut slots| slots.get_mut(position).and_then(|slots| slots.get_mut(self.index).and_then(Option::take))) else { continue; };
            let relay = Relay::new(Arc::new(settings.clone()), self.state.clone())?;

            Rt::spawn_local(relay.datagrams(socket, self.stop.clone()));

        }

        for ( position, settings ) in self.config.streams.iter().enumerate().filter(|( _, settings )| !settings.udp) {

            let Some(listener) = self.streams.lock().ok().and_then(|mut slots| slots.get_mut(position).and_then(|slots| slots.get_mut(self.index).and_then(Option::take))) else { continue; };
            let relay = Relay::new(Arc::new(settings.clone()), self.state.clone())?;

            Rt::spawn_local(relay.serve(listener, self.stop.clone()));

        }

        let mut extras = Vec::new();

        for ( position, extra ) in self.config.listeners.iter().enumerate() {

            let Some(listener) = self.extras.lock().ok().and_then(|mut slots| slots.get_mut(position).and_then(|slots| slots.get_mut(self.index).and_then(Option::take))) else { continue; };

            if extra.redirect { self.redirect(listener, self.state.tokens(), extra.proxy_protocol); continue; }

            let handler = handler.clone();
            let connect = { let handler = handler.clone(); move |peer, secure| handler.connect(peer, secure) };
            let server = Server::new(self.config.server_settings(extra.proxy_protocol)).tls(if extra.tls { self.state.tls() } else { Swap::new(None) });
            let stop = self.stop.clone();

            extras.push(Rt::spawn_local(async move {

                if let Err(error) = server.serve(listener, stop, connect, move |request, context| { let handler = handler.clone(); async move { handler.handle(request, context).await } }).await { warn!(%error, "listener stopped"); }

            }));

        }

        debug!(worker = self.index, "worker ready");

        let connect = { let handler = handler.clone(); move |peer, secure| handler.connect(peer, secure) };

        let socket = self.quic.lock().ok().and_then(|mut slots| slots.get_mut(self.index).and_then(Option::take));

        let quic = match ( self.config.quic_settings(), self.state.tls().load().as_ref(), socket ) {
            ( Some(settings), Some(acceptor), Some(socket) ) => Some(Quic::endpoint(settings, acceptor.quic(), socket)?),
            _ => None,
        };

        if let Some(endpoint) = quic {

            let handler = handler.clone();
            let connect = { let handler = handler.clone(); move |peer, secure| handler.connect(peer, secure) };
            let stop = self.stop.clone();

            let ( watched, tls, settings ) = ( endpoint.clone(), self.state.tls(), self.config.quic_settings() );

            Rt::spawn_local(async move {

                let mut known = Arc::as_ptr(&tls.load()) as usize;

                loop {

                    Rt::sleep(1_000).await;

                    let current = tls.load();

                    if Arc::as_ptr(&current) as usize == known { continue; }

                    known = Arc::as_ptr(&current) as usize;

                    if let ( Some(settings), Some(acceptor) ) = ( settings, current.as_ref() ) && let Ok(server) = Quic::server(&settings, acceptor.quic()) { watched.set_server_config(Some(server)); }

                }

            });

            Rt::spawn_local(async move {

                if let Err(error) = Quic::serve(endpoint, stop, connect, move |request, context| { let handler = handler.clone(); async move { handler.handle(request, context).await } }).await { warn!(%error, "http/3 listener stopped"); }

            });

        }

        let served = Server::new(self.config.server_settings(self.config.server.proxy_protocol)).tls(self.state.tls()).serve(listener, self.stop, connect, move |request, context| {

            let handler = handler.clone();

            async move { handler.handle(request, context).await }

        }).await;

        for extra in extras { let _ = extra.await; }

        let deadline = Instant::now() + Duration::from_millis(self.config.server.drain_ms);

        while bridges.tunnels() > 0 && Instant::now() < deadline { Rt::sleep(20).await; }

        served

    }

}

impl Worker {

    fn challenge_listener ( &self, addr: SocketAddr, tokens: Tokens ) -> AppResult<()> {

        let listener = Listener::bind(addr, 128, 1, Accept::Shared)?.pop().ok_or_else(|| AppError::message("acme challenge listener missing"))?;

        self.redirect(listener, Some(tokens), false);

        Ok(())

    }

    fn redirect ( &self, listener: Listener, tokens: Option<Tokens>, proxy_protocol: bool ) {

        let settings = self.config.server_settings(proxy_protocol);
        let stop = self.stop.clone();

        Rt::spawn_local(async move {

            let connect = |peer: SocketAddr, _: bool| peer;
            let handler = move |request: Box<Req<Body>>, _: Rc<SocketAddr>| {

                let tokens = tokens.clone();

                async move { Self::challenge(tokens.as_ref(), &request) }

            };

            if let Err(error) = Server::new(settings).serve(listener, stop, connect, handler).await { warn!(%error, "redirect listener stopped"); }

        });

    }

    fn challenge ( tokens: Option<&Tokens>, request: &Req<Body> ) -> Res<Body> {

        let path = request.uri().path();

        if let Some(tokens) = tokens && let Some(token) = path.strip_prefix("/.well-known/acme-challenge/") && let Some(value) = tokens.read().ok().and_then(|map| map.get(token).cloned()) { return Response::bytes(200, "text/plain", value); }

        let host = request.headers().get(HOST).and_then(|value| value.to_str().ok()).map(|host| host.split(':').next().unwrap_or(host)).unwrap_or("");
        let target = request.uri().path_and_query().map_or("/", |target| target.as_str());
        let mut response = Response::status(308);

        if let Ok(location) = http::header::HeaderValue::from_str(&format!("https://{host}{target}")) { response.headers_mut().insert(LOCATION, location); }

        response

    }

}
