use std::cell::{Cell, RefCell};
use std::future::Future;
use std::net::SocketAddr;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::task::Waker;
use std::time::{Duration, Instant};

use http::StatusCode;
use http::header::{CONNECTION, HeaderValue};
use hyper::server::conn::{http1, http2};
use hyper::service::service_fn;
use tokio::task::AbortHandle;

use crate::core::error::{AppError, AppResult};
use crate::core::log::{debug, warn};
use crate::core::rt::Rt;
use crate::core::sync::{Local, Swap, Watch};
use crate::core::time::Clock;
use crate::http::body::Body;
use crate::http::io::{Io, LocalExec};
use crate::http::request::Req;
use crate::http::response::{Abort, Res};
use crate::http::tls::{ALPN_HTTP2, Acceptor};
use super::arch::{CLOSING, DRAINING, FLUSH_MAX, GRACE_MS, Graceful, HYPER_MAX_HEADERS, Listener, Server, Session, Sessions, Settings, Slot, Watched};

impl Server {

    pub fn new ( settings: Settings ) -> Self {

        Self { settings, tls: Swap::new(None) }

    }

    pub fn tls ( self, tls: Swap<Option<Acceptor>> ) -> Self {

        Self { tls, ..self }

    }

    pub async fn serve <C, M, H, F> ( self, mut listener: Listener, mut stop: Watch, connect: M, handler: H ) -> AppResult<()>
    where C: Session + 'static, M: Fn(SocketAddr, bool) -> C + 'static, H: Fn(Box<Req<Body>>, Rc<C>) -> F + Clone + 'static, F: Future<Output = Res<Body>> + 'static {

        let Self { settings, tls } = self;
        let active = Local::new(0usize);
        let sessions: Sessions<C> = Rc::new(RefCell::new(Vec::new()));
        let stage = Rc::new(Cell::new(0u8));
        let sweeper = Rt::spawn_local(Self::sweep(sessions.clone(), settings.header_timeout_ms, settings.keepalive_timeout_ms, settings.send_timeout_ms));

        let mut plain = http1::Builder::new();

        plain.keep_alive(settings.keepalive).max_buf_size(settings.buffer).pipeline_flush(true).timer(Io::timer()).header_read_timeout(None);

        if settings.max_headers != HYPER_MAX_HEADERS { plain.max_headers(settings.max_headers); }

        let mut multi = http2::Builder::new(LocalExec);

        multi.max_concurrent_streams(settings.max_streams)
            .adaptive_window(settings.h2_adaptive_window)
            .initial_stream_window_size(settings.h2_stream_window)
            .initial_connection_window_size(settings.h2_connection_window)
            .max_frame_size(settings.h2_max_frame)
            .max_header_list_size(settings.h2_max_header_bytes)
            .enable_connect_protocol()
            .keep_alive_interval(Duration::from_secs(20))
            .keep_alive_timeout(Duration::from_secs(20))
            .timer(Io::timer());

        let engines = Rc::new(( plain, multi ));
        let connect = Rc::new(connect);
        let cleartext = settings.http2 && settings.h2c;

        let mut stopping = false;
        let mut flushed = 0usize;

        loop {

            let accepted = match stopping {
                true if flushed >= FLUSH_MAX => break,
                true => match tokio::time::timeout(Duration::ZERO, listener.accept()).await { Ok(accepted) => { flushed += 1; accepted } Err(_) => break },
                false => tokio::select! {
                    accepted = listener.accept() => accepted,
                    _ = stop.wait() => { stopping = true; continue; }
                },
            };

            let ( stream, peer ) = match accepted {
                Some(Ok(accepted)) => accepted,
                None => break,
                Some(Err(error)) => {

                    warn!(%error, "accept failed");

                    if stopping { break; }

                    Rt::sleep(10).await;

                    continue;

                }
            };

            if settings.max_connections > 0 && active.with(|count| *count) >= settings.max_connections {

                debug!(%peer, limit = settings.max_connections, "connection refused at the connection limit");

                continue;

            }

            stream.tune();

            let acceptor = tls.load();
            let handler = handler.clone();
            let slot = Slot::claim(&active);
            let engines = engines.clone();
            let connect = connect.clone();
            let registry = sessions.clone();
            let claim = Rc::new(Cell::new(None::<AbortHandle>));
            let claimed = claim.clone();
            let stall = Arc::new(AtomicU64::new(0));
            let stage = stage.clone();
            let wake = Rc::new(Cell::new(None::<Waker>));

            let task = Rt::spawn_local(async move {

                let _slot = slot;
                let mut stream = stream;

                let peer = match settings.proxy_protocol {
                    true => match Rt::timeout("proxy protocol", settings.header_timeout_ms, stream.announced(peer)).await.and_then(|source| source) {
                        Ok(source) => source,
                        Err(error) => {

                            debug!(%peer, %error, "connection ended");

                            return;

                        }
                    },
                    false => peer,
                };

                let context = Rc::new(connect(peer, acceptor.is_some()));
                let session = context.clone();

                if let Some(abort) = claim.take() { registry.borrow_mut().push(( Rc::downgrade(&context), abort, stall.clone(), wake.clone() )); }

                let stream = Watched::new(stream, stall);
                let leaving = stage.clone();

                let service = service_fn(move |request: http::Request<hyper::body::Incoming>| {

                    let pending = Box::pin(handler(Box::new(request.map(Body::incoming)), context.clone()));
                    let last = leaving.get() != 0;

                    async move {

                        let mut response = pending.await;

                        if last && response.status() != StatusCode::SWITCHING_PROTOCOLS { response.headers_mut().insert(CONNECTION, HeaderValue::from_static("close")); }

                        if response.extensions().get::<Abort>().is_some() { return Err(std::io::Error::other("the route drops the connection")); }

                        Ok::<Res<Body>, std::io::Error>(response)

                    }

                });

                let outcome = match acceptor.as_ref() {
                    Some(acceptor) => match Box::pin(acceptor.accept(stream)).await {
                        Ok(stream) => {

                            if let Some(certificate) = stream.get_ref().1.peer_certificates().and_then(|chain| chain.first()) { session.identify(certificate.as_ref()); }

                            let served = match stream.get_ref().1.alpn_protocol() == Some(ALPN_HTTP2) {
                                true => Graceful::new(engines.1.serve_connection(Io::wrap(stream), service), |connection| connection.graceful_shutdown(), DRAINING, &stage, &wake).await,
                                false => Graceful::new(engines.0.serve_connection(Io::wrap(stream), service).with_upgrades(), |connection| connection.graceful_shutdown(), CLOSING, &stage, &wake).await,
                            };

                            served.map_err(|error| AppError::message(error.to_string()))

                        }
                        Err(error) => Err(error),
                    },
                    None => match cleartext && stream.prefaced().await {
                        true => Graceful::new(engines.1.serve_connection(Io::wrap(stream), service), |connection| connection.graceful_shutdown(), DRAINING, &stage, &wake).await,
                        false => Graceful::new(engines.0.serve_connection(Io::wrap(stream), service).with_upgrades(), |connection| connection.graceful_shutdown(), CLOSING, &stage, &wake).await,
                    }.map_err(|error| AppError::message(error.to_string())),
                };

                if let Err(error) = outcome { debug!(%peer, %error, "connection ended"); }

            });

            claimed.set(Some(task.abort_handle()));

        }

        drop(listener);

        Self::drain(&active, &sessions, &stage, settings.drain_ms).await;
        sweeper.abort();

        Ok(())

    }

    async fn sweep <C: Session> ( sessions: Sessions<C>, header_ms: u64, keepalive_ms: u64, send_ms: u64 ) {

        let tick = header_ms.min(keepalive_ms).min(send_ms.max(1)).div_ceil(4).clamp(100, 1_000);

        loop {

            Rt::sleep(tick).await;

            let now = Clock::tick();
            let stamp = Clock::stamp(now);

            sessions.borrow_mut().retain(|( session, abort, stall, _ )| {

                let Some(session) = session.upgrade() else { return false; };
                let blocked = stall.load(Ordering::Relaxed);

                if send_ms > 0 && blocked != 0 && stamp.saturating_sub(blocked) > send_ms { abort.abort(); return false; }

                let Some(( since, served )) = session.idle() else { return true; };
                let allowed = if served { keepalive_ms } else { header_ms };

                if now.saturating_duration_since(since).as_millis() as u64 <= allowed { return true; }

                abort.abort();

                false

            });

        }

    }

    async fn drain <C> ( active: &Local<usize>, sessions: &Sessions<C>, stage: &Cell<u8>, millis: u64 ) {

        let started = Instant::now();

        for ( phase, limit ) in [( DRAINING, millis.min(GRACE_MS) ), ( CLOSING, millis )] {

            stage.set(phase);

            for ( _, _, _, waker ) in sessions.borrow().iter() {

                if let Some(held) = waker.take() { held.wake_by_ref(); waker.set(Some(held)); }

            }

            while active.with(|count| *count > 0) && (started.elapsed().as_millis() as u64) < limit { Rt::sleep(10).await; }

        }

    }

}
