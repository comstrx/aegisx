use std::pin::Pin;
use std::task::{Context, Poll, Waker};
use std::time::{Duration, Instant};

use bytes::Bytes;
use http::Uri;
use http::header::{CONNECTION, HOST, HeaderValue};
use http::uri::{Authority, PathAndQuery, Scheme};
use http_body::{Body as _, Frame};
use http_body_util::BodyExt;
use hyper::upgrade::OnUpgrade;
use hyper_util::client::legacy;

use crate::core::arena::Arena;
use crate::core::error::{AppError, AppResult};
use crate::core::net::Address;
use crate::core::pool::Slot;
use crate::core::sync::Local;
use crate::http::body::{Body, Incoming};
use crate::http::header::Header;
use crate::http::io::{Io as Runtime, LocalExec};
use crate::http::request::{Req, Request, Shadow};
use crate::http::response::Res;
use crate::http::tls::Trust;
use super::arch::{Client, Connector, Failure, Outbound, PROBE_BYTES, Peeked, Protocol, Replay, Settings, Streaming, Timing, Upstream, Wire};

impl Upstream {

    pub fn new ( addr: Address, protocol: Protocol ) -> Self {

        let text = match &addr { Address::Tcp(socket) => socket.to_string(), Address::Unix(_) => "localhost".to_string(), Address::Name(host, port) => format!("{host}:{port}") };

        Self { key: ( addr.clone(), false ), slot: Slot::of(&format!("tcp {addr}")), authority: Self::authority(&text), scheme: Scheme::HTTP, origin: Self::origin(&text), addr, trust: None, protocol }

    }

    pub fn secure ( addr: Address, trust: Trust, protocol: Protocol ) -> Self {

        let host = trust.host();
        let text = match addr.port() { Some(443) | None => host, Some(port) => format!("{host}:{port}") };

        Self { key: ( addr.clone(), true ), slot: Slot::of(&format!("tls {addr}")), authority: Self::authority(&text), scheme: Scheme::HTTPS, origin: Self::origin(&text), addr, trust: Some(trust), protocol }

    }

    fn authority ( text: &str ) -> HeaderValue {

        Header::static_value(text).unwrap_or_else(|| HeaderValue::from_static("localhost"))

    }

    fn origin ( text: &str ) -> Authority {

        Authority::try_from(text).unwrap_or_else(|_| Authority::from_static("localhost"))

    }

}

impl Client {

    pub fn new ( settings: Settings ) -> Self {

        Self { wires: Local::new(Vec::new()), arena: Local::new(Arena::new()), settings }

    }

    fn wire ( upstream: &Upstream, settings: &Settings ) -> Wire {

        let mut builder = legacy::Client::builder(LocalExec);

        builder
            .pool_idle_timeout(Duration::from_millis(settings.pool_idle_ms.max(1)))
            .pool_max_idle_per_host(settings.pool_capacity)
            .pool_timer(Runtime::timer())
            .timer(Runtime::timer())
            .retry_canceled_requests(true)
            .set_host(false)
            .http1_max_buf_size(settings.buffer.max(8_192))
            .http1_writev(true)
            .http2_only(upstream.protocol == Protocol::Http2 && upstream.trust.is_none());

        builder.build(Connector { addr: upstream.addr.clone(), trust: upstream.trust.clone(), protocol: upstream.protocol, timeout_ms: settings.connect_timeout_ms })

    }

    pub async fn exchange ( &self, upstream: &Upstream, timing: Timing, replay: Replay, request: Box<Req<Body>>, prepare: impl Fn(&mut Req<Body>) -> Option<Uri> ) -> Result<( Res<Body>, Option<Shadow> ), Failure> {

        let mut outgoing = request;
        let shadow = (replay.status || replay.failure).then(|| self.arena.with_mut(|arena| Request::shadow(&outgoing, arena))).flatten();
        let restored = |shadow: &Option<Shadow>| shadow.as_ref().and_then(Shadow::restore);

        prepare(&mut outgoing);

        let ( mut head, body ) = (*outgoing).into_parts();
        let mut target = http::uri::Parts::default();

        target.scheme = Some(upstream.scheme.clone());
        target.authority = Some(upstream.origin.clone());
        target.path_and_query = Some(head.uri.path_and_query().cloned().unwrap_or_else(|| PathAndQuery::from_static("/")));

        head.uri = match Uri::from_parts(target) {
            Ok(uri) => uri,
            Err(error) => return Err(Failure { error: AppError::invalid("upstream uri", error.to_string()), request: restored(&shadow), connect: false }),
        };

        let pending = self.wires.with_mut(|wires| {

            if wires.len() <= upstream.slot { wires.resize_with(upstream.slot + 1, || None); }

            wires[upstream.slot].get_or_insert_with(|| Self::wire(upstream, &self.settings)).request(Req::from_parts(head, Outbound::new(body)))

        });

        let deadline = tokio::time::Instant::from_std(timing.started) + Duration::from_millis(timing.timeout_ms.max(1));

        let response = match tokio::time::timeout_at(deadline, pending).await {
            Ok(Ok(response)) => response,
            Ok(Err(error)) => return Err(Failure { connect: error.is_connect(), error: Self::cause(&error, upstream), request: restored(&shadow) }),
            Err(_) => return Err(Failure { error: AppError::timeout("upstream response", timing.timeout_ms), request: restored(&shadow), connect: false }),
        };

        let ( mut head, mut incoming ) = response.into_parts();
        let upgrade = (head.status == http::StatusCode::SWITCHING_PROTOCOLS).then(|| head.extensions.remove::<OnUpgrade>()).flatten();

        let body = match upgrade {
            Some(upgrade) => Body::upstream(Streaming::new(incoming, timing.timeout_ms).upgradable(Some(upgrade))),
            None => match Self::peek(&mut incoming) {
                Ok(Peeked::Whole(data)) => data.map_or(Body::Empty, Body::bytes),
                Ok(Peeked::Partial(first)) => Body::upstream(Streaming::new(incoming, timing.timeout_ms).prefixed(first)),
                Err(error) => return Err(Failure { error: AppError::network(upstream.addr.to_string(), error.to_string()), request: restored(&shadow), connect: false }),
            },
        };

        Ok(( Res::from_parts(head, body), shadow.filter(|_| replay.status) ))

    }

    pub async fn probe ( &self, upstream: &Upstream, path: Option<&str>, timeout_ms: u64 ) -> AppResult<Option<( u16, Bytes )>> {

        let Some(path) = path else {

            Connector { addr: upstream.addr.clone(), trust: upstream.trust.clone(), protocol: upstream.protocol, timeout_ms }.open().await?;

            return Ok(None);

        };

        let request = http::Request::builder().method(http::Method::GET).uri(path).header(HOST, upstream.authority.clone()).header(CONNECTION, HeaderValue::from_static("close")).body(Body::Empty).map_err(|error| AppError::invalid("probe request", error.to_string()))?;
        let ( response, _ ) = self.exchange(upstream, Timing { started: Instant::now(), timeout_ms }, Replay::default(), Box::new(request), |_| None).await.map_err(|failure| failure.error)?;
        let status = response.status().as_u16();
        let body = http_body_util::Limited::new(response.into_body(), PROBE_BYTES).collect().await.map(|collected| collected.to_bytes()).unwrap_or_default();

        Ok(Some(( status, body )))

    }

    fn peek ( incoming: &mut Incoming ) -> Result<Peeked, hyper::Error> {

        if incoming.is_end_stream() { return Ok(Peeked::Whole(None)); }

        match Pin::new(&mut *incoming).poll_frame(&mut Context::from_waker(Waker::noop())) {
            Poll::Ready(Some(Ok(frame))) => match frame.into_data() {
                Ok(data) if incoming.is_end_stream() => Ok(Peeked::Whole(Some(data))),
                Ok(data) => Ok(Peeked::Partial(Some(Frame::data(data)))),
                Err(frame) => Ok(Peeked::Partial(Some(frame))),
            },
            Poll::Ready(Some(Err(error))) => Err(error),
            Poll::Ready(None) => Ok(Peeked::Whole(None)),
            Poll::Pending => Ok(Peeked::Partial(None)),
        }

    }

    fn cause ( error: &legacy::Error, upstream: &Upstream ) -> AppError {

        let mut source = std::error::Error::source(error);

        while let Some(inner) = source {

            if let Some(app) = inner.downcast_ref::<AppError>() { return match app { AppError::Http { .. } => AppError::http(app.status(), app.to_string()), _ => AppError::network(upstream.addr.to_string(), app.to_string()) }; }

            source = inner.source();

        }

        AppError::network(upstream.addr.to_string(), error.to_string())

    }

}
