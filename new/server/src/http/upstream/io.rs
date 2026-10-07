use std::io::IoSlice;
use std::pin::Pin;
use std::task::{Context, Poll};

use std::future::Future;

use http::Uri;
use hyper_util::client::legacy::connect::{Connected, Connection};
use hyper_util::rt::TokioIo;
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tokio::net::TcpStream;

use crate::core::error::{AppError, AppResult};
use crate::core::net::{Address, Socket};
use crate::core::rt::Rt;
use crate::http::tls::ALPN_HTTP2;
use super::arch::{Connector, Io, Linked, Protocol};

impl Connector {

    pub async fn open ( &self ) -> AppResult<Linked> {

        let addr = match &self.addr {
            Address::Tcp(addr) => *addr,
            #[cfg(unix)]
            Address::Unix(path) => {

                if self.trust.is_some() { return Err(AppError::unsupported("tls over unix sockets")); }

                let stream = Rt::timeout("upstream connect", self.timeout_ms, tokio::net::UnixStream::connect(path.as_ref())).await?.map_err(|error| AppError::network(self.addr.to_string(), error.to_string()))?;

                return Ok(Linked { io: TokioIo::new(Io::Unix(stream)), h2: self.protocol == Protocol::Http2 });

            }
            #[cfg(not(unix))]
            Address::Unix(_) => return Err(AppError::unsupported("unix sockets on this platform")),
            Address::Name(host, _) => return Err(AppError::network(host.to_string(), "backend name was not resolved")),
        };

        let stream = Rt::timeout("upstream connect", self.timeout_ms, TcpStream::connect(addr)).await?.map_err(|error| AppError::network(addr.to_string(), error.to_string()))?;

        Socket::tune(&stream);

        match &self.trust {
            Some(trust) => {

                let tls = Rt::timeout("upstream tls handshake", self.timeout_ms, trust.connect(stream)).await??;
                let negotiated = tls.get_ref().1.alpn_protocol() == Some(ALPN_HTTP2);
                let h2 = match self.protocol { Protocol::Http2 => true, Protocol::Http1 | Protocol::Fastcgi => false, Protocol::Auto => negotiated };

                Ok(Linked { io: TokioIo::new(Io::Tls(Box::new(tls))), h2 })

            }
            None => Ok(Linked { io: TokioIo::new(Io::Plain(stream)), h2: self.protocol == Protocol::Http2 }),
        }

    }

}

impl tower_service::Service<Uri> for Connector {

    type Response = Linked;
    type Error = AppError;
    type Future = Pin<Box<dyn Future<Output = AppResult<Linked>> + Send>>;

    fn poll_ready ( &mut self, _: &mut Context<'_> ) -> Poll<AppResult<()>> {

        Poll::Ready(Ok(()))

    }

    fn call ( &mut self, _: Uri ) -> Self::Future {

        let connector = self.clone();

        Box::pin(async move { connector.open().await })

    }

}

impl Connection for Linked {

    fn connected ( &self ) -> Connected {

        match self.h2 { true => Connected::new().negotiated_h2(), false => Connected::new() }

    }

}

impl hyper::rt::Read for Linked {

    fn poll_read ( self: Pin<&mut Self>, cx: &mut Context<'_>, buf: hyper::rt::ReadBufCursor<'_> ) -> Poll<std::io::Result<()>> {

        Pin::new(&mut self.get_mut().io).poll_read(cx, buf)

    }

}

impl hyper::rt::Write for Linked {

    fn poll_write ( self: Pin<&mut Self>, cx: &mut Context<'_>, buf: &[u8] ) -> Poll<std::io::Result<usize>> {

        Pin::new(&mut self.get_mut().io).poll_write(cx, buf)

    }

    fn poll_flush ( self: Pin<&mut Self>, cx: &mut Context<'_> ) -> Poll<std::io::Result<()>> {

        Pin::new(&mut self.get_mut().io).poll_flush(cx)

    }

    fn poll_shutdown ( self: Pin<&mut Self>, cx: &mut Context<'_> ) -> Poll<std::io::Result<()>> {

        Pin::new(&mut self.get_mut().io).poll_shutdown(cx)

    }

    fn is_write_vectored ( &self ) -> bool {

        self.io.is_write_vectored()

    }

    fn poll_write_vectored ( self: Pin<&mut Self>, cx: &mut Context<'_>, bufs: &[IoSlice<'_>] ) -> Poll<std::io::Result<usize>> {

        Pin::new(&mut self.get_mut().io).poll_write_vectored(cx, bufs)

    }

}

impl AsyncRead for Io {

    fn poll_read ( self: Pin<&mut Self>, cx: &mut Context<'_>, buf: &mut ReadBuf<'_> ) -> Poll<std::io::Result<()>> {

        match self.get_mut() {
            Io::Plain(stream) => Pin::new(stream).poll_read(cx, buf),
            Io::Tls(stream) => Pin::new(stream.as_mut()).poll_read(cx, buf),
            #[cfg(unix)]
            Io::Unix(stream) => Pin::new(stream).poll_read(cx, buf),
        }

    }

}

impl AsyncWrite for Io {

    fn poll_write ( self: Pin<&mut Self>, cx: &mut Context<'_>, buf: &[u8] ) -> Poll<std::io::Result<usize>> {

        match self.get_mut() {
            Io::Plain(stream) => Pin::new(stream).poll_write(cx, buf),
            Io::Tls(stream) => Pin::new(stream.as_mut()).poll_write(cx, buf),
            #[cfg(unix)]
            Io::Unix(stream) => Pin::new(stream).poll_write(cx, buf),
        }

    }

    fn poll_write_vectored ( self: Pin<&mut Self>, cx: &mut Context<'_>, bufs: &[IoSlice<'_>] ) -> Poll<std::io::Result<usize>> {

        match self.get_mut() {
            Io::Plain(stream) => Socket::gather(Pin::new(stream), cx, bufs),
            Io::Tls(stream) => Pin::new(stream.as_mut()).poll_write_vectored(cx, bufs),
            #[cfg(unix)]
            Io::Unix(stream) => Socket::gather(Pin::new(stream), cx, bufs),
        }

    }

    fn is_write_vectored ( &self ) -> bool {

        match self {
            Io::Plain(stream) => stream.is_write_vectored(),
            Io::Tls(stream) => stream.is_write_vectored(),
            #[cfg(unix)]
            Io::Unix(stream) => stream.is_write_vectored(),
        }

    }

    fn poll_flush ( self: Pin<&mut Self>, cx: &mut Context<'_> ) -> Poll<std::io::Result<()>> {

        match self.get_mut() {
            Io::Plain(stream) => Pin::new(stream).poll_flush(cx),
            Io::Tls(stream) => Pin::new(stream.as_mut()).poll_flush(cx),
            #[cfg(unix)]
            Io::Unix(stream) => Pin::new(stream).poll_flush(cx),
        }

    }

    fn poll_shutdown ( self: Pin<&mut Self>, cx: &mut Context<'_> ) -> Poll<std::io::Result<()>> {

        match self.get_mut() {
            Io::Plain(stream) => Pin::new(stream).poll_shutdown(cx),
            Io::Tls(stream) => Pin::new(stream.as_mut()).poll_shutdown(cx),
            #[cfg(unix)]
            Io::Unix(stream) => Pin::new(stream).poll_shutdown(cx),
        }

    }

}
