use std::future::Future;
use std::net::SocketAddr;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

use bytes::{Buf, Bytes};
use http_body_util::BodyExt;
use quinn::crypto::rustls::QuicServerConfig;
use quinn::{Endpoint, EndpointConfig, IdleTimeout, ServerConfig, TransportConfig, VarInt};

use crate::core::error::{AppError, AppFail, AppResult};
use crate::core::net::Socket;
use crate::core::log::{debug, warn};
use crate::core::rt::Rt;
use crate::core::sync::Watch;
use crate::http::body::Body;
use crate::http::request::Req;
use crate::http::response::Res;
use super::arch::{Congestion, Quic, QuicSettings};

impl Quic {

    pub fn bind ( settings: &QuicSettings, workers: usize ) -> AppResult<Vec<std::net::UdpSocket>> {

        (0..if settings.shared { workers.max(1) } else { 1 }).map(|_| Socket::datagram(settings.listen, settings.shared)).collect()

    }

    pub fn endpoint ( settings: QuicSettings, tls: Arc<rustls::ServerConfig>, socket: std::net::UdpSocket ) -> AppResult<Endpoint> {

        Endpoint::new(EndpointConfig::default(), Some(Self::server(&settings, tls)?), socket, Arc::new(quinn::TokioRuntime)).or_fail("cannot start quic endpoint")

    }

    pub fn server ( settings: &QuicSettings, tls: Arc<rustls::ServerConfig> ) -> AppResult<ServerConfig> {

        let crypto = QuicServerConfig::try_from(tls).map_err(|error| AppError::config("set_http3", format!("tls configuration is not usable for quic: {error}")))?;
        let mut server = ServerConfig::with_crypto(Arc::new(crypto));
        let mut transport = TransportConfig::default();

        transport.max_idle_timeout(Some(IdleTimeout::try_from(Duration::from_millis(settings.max_idle_ms)).map_err(|_| AppError::config("set_http3", "max_idle_ms is too large"))?));
        transport.max_concurrent_bidi_streams(VarInt::from_u32(settings.max_streams));
        transport.stream_receive_window(VarInt::from_u64(settings.stream_window).map_err(|_| AppError::config("set_http3", "stream_window is too large"))?);
        transport.send_window(settings.send_window);

        match settings.congestion {
            Congestion::Cubic => {}
            Congestion::Bbr => { transport.congestion_controller_factory(Arc::new(quinn::congestion::BbrConfig::default())); }
            Congestion::NewReno => { transport.congestion_controller_factory(Arc::new(quinn::congestion::NewRenoConfig::default())); }
        }

        server.transport_config(Arc::new(transport));

        Ok(server)

    }

    pub async fn serve <C, M, H, F> ( endpoint: Endpoint, mut stop: Watch, connect: M, handler: H ) -> AppResult<()>
    where C: 'static, M: Fn(SocketAddr, bool) -> C + 'static, H: Fn(Box<Req<Body>>, Rc<C>) -> F + Clone + 'static, F: Future<Output = Res<Body>> + 'static {

        loop {

            let incoming = tokio::select! {
                incoming = endpoint.accept() => incoming,
                _ = stop.wait() => break,
            };

            let Some(incoming) = incoming else { break; };
            let peer = incoming.remote_address();
            let context = Rc::new(connect(peer, true));
            let handler = handler.clone();

            Rt::spawn_local(async move {

                let connection = match incoming.await {
                    Ok(connection) => connection,
                    Err(error) => { debug!(%peer, %error, "quic handshake failed"); return; }
                };

                let mut session = match h3::server::Connection::<_, Bytes>::new(h3_quinn::Connection::new(connection)).await {
                    Ok(session) => session,
                    Err(error) => { debug!(%peer, %error, "h3 setup failed"); return; }
                };

                loop {

                    let resolver = match session.accept().await {
                        Ok(Some(resolver)) => resolver,
                        Ok(None) => break,
                        Err(error) => { debug!(%peer, %error, "h3 connection ended"); break; }
                    };

                    let handler = handler.clone();
                    let context = context.clone();

                    Rt::spawn_local(async move {

                        let ( request, stream ) = match resolver.resolve_request().await {
                            Ok(resolved) => resolved,
                            Err(error) => { debug!(%peer, %error, "h3 request rejected"); return; }
                        };

                        let ( mut send, mut recv ) = stream.split();
                        let bodiless = !request.headers().contains_key(http::header::CONTENT_LENGTH) && matches!(*request.method(), http::Method::GET | http::Method::HEAD | http::Method::OPTIONS | http::Method::DELETE);

                        let body = match bodiless {
                            true => match recv.recv_data().await {
                                Ok(None) => Body::Empty,
                                Ok(Some(mut first)) => Body::chained(first.copy_to_bytes(first.remaining()), Body::quic(recv)),
                                Err(error) => { debug!(%peer, %error, "h3 request body failed"); return; }
                            },
                            false => Body::quic(recv),
                        };

                        let request = Box::new(request.map(|()| body));
                        let response = handler(request, context).await;
                        let ( head, mut body ) = response.into_parts();

                        if let Err(error) = send.send_response(http::Response::from_parts(head, ())).await { debug!(%peer, %error, "h3 response head failed"); return; }

                        while let Some(frame) = body.frame().await {

                            let frame = match frame { Ok(frame) => frame, Err(error) => { warn!(%peer, %error, "h3 body failed"); return; } };

                            match frame.into_data() {
                                Ok(data) => { if let Err(error) = send.send_data(data).await { debug!(%peer, %error, "h3 send failed"); return; } }
                                Err(frame) => { if let Ok(trailers) = frame.into_trailers() && let Err(error) = send.send_trailers(trailers).await { debug!(%peer, %error, "h3 trailers failed"); return; } }
                            }

                        }

                        if let Err(error) = send.finish().await { debug!(%peer, %error, "h3 finish failed"); }

                    });

                }

            });

        }

        endpoint.close(VarInt::from_u32(0), b"shutdown");
        endpoint.wait_idle().await;

        Ok(())

    }

}
