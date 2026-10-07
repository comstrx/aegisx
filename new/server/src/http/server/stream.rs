use std::io::IoSlice;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::task::{Context, Poll};

use std::net::SocketAddr;

use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, ReadBuf};

use crate::core::error::{AppError, AppFail, AppResult};
use crate::core::net::{Address, Announced, Preamble, Socket};
use crate::core::rt::Rt;
use crate::core::time::Clock;
use crate::http::tls::Tls;
use super::arch::{HELLO_BYTES, PREAMBLE_BYTES, PREFACE, Stream, Watched};

impl Stream {

    pub async fn dial ( address: &Address, timeout_ms: u64 ) -> AppResult<Self> {

        match address {
            Address::Tcp(addr) => {

                let stream = Rt::timeout("connect", timeout_ms, tokio::net::TcpStream::connect(addr)).await?.map_err(|error| AppError::network(addr.to_string(), error.to_string()))?;

                Socket::tune(&stream);

                Ok(Self::Tcp(stream))

            }
            #[cfg(unix)]
            Address::Unix(path) => Ok(Self::Unix(Rt::timeout("connect", timeout_ms, tokio::net::UnixStream::connect(path.as_ref())).await?.map_err(|error| AppError::network(address.to_string(), error.to_string()))?)),
            #[cfg(not(unix))]
            Address::Unix(_) => Err(AppError::unsupported("unix sockets on this platform")),
            Address::Name(host, _) => Err(AppError::network(host.to_string(), "backend name was not resolved")),
        }

    }

    pub fn tune ( &self ) {

        if let Self::Tcp(stream) = self { Socket::tune(stream); }

    }

    pub async fn announced ( &mut self, peer: SocketAddr ) -> AppResult<SocketAddr> {

        let Self::Tcp(stream) = self else { return Ok(peer); };
        let mut seen = vec![0u8; PREAMBLE_BYTES];

        loop {

            let count = stream.peek(&mut seen).await.or_fail("cannot read the proxy protocol header")?;

            match Preamble::read(&seen[..count]) {
                Announced::Done { length, source } => {

                    stream.read_exact(&mut seen[..length]).await.or_fail("cannot consume the proxy protocol header")?;

                    return Ok(source.unwrap_or(peer));

                }
                Announced::Partial if count > 0 && count < seen.len() => Rt::sleep(2).await,
                _ => return Err(AppError::invalid("proxy protocol", "the connection did not start with a valid header")),
            }

        }

    }

    pub async fn server_name ( &self ) -> Option<String> {

        let Self::Tcp(stream) = self else { return None; };
        let mut seen = vec![0u8; HELLO_BYTES];

        loop {

            let count = stream.peek(&mut seen).await.ok()?;

            match Tls::server_name(&seen[..count]) {
                Some(name) => return name,
                None if count > 0 && count < seen.len() => Rt::sleep(2).await,
                None => return None,
            }

        }

    }

    pub async fn prefaced ( &self ) -> bool {

        let Self::Tcp(stream) = self else { return false; };
        let mut seen = [0u8; 24];

        match stream.peek(&mut seen).await {
            Ok(count) => count >= 3 && PREFACE.starts_with(&seen[..count]),
            Err(_) => false,
        }

    }

}

impl AsyncRead for Stream {

    fn poll_read ( self: Pin<&mut Self>, cx: &mut Context<'_>, buf: &mut ReadBuf<'_> ) -> Poll<std::io::Result<()>> {

        match self.get_mut() {
            Self::Tcp(stream) => Pin::new(stream).poll_read(cx, buf),
            #[cfg(unix)]
            Self::Unix(stream) => Pin::new(stream).poll_read(cx, buf),
        }

    }

}

impl AsyncWrite for Stream {

    fn poll_write ( self: Pin<&mut Self>, cx: &mut Context<'_>, buf: &[u8] ) -> Poll<std::io::Result<usize>> {

        match self.get_mut() {
            Self::Tcp(stream) => Pin::new(stream).poll_write(cx, buf),
            #[cfg(unix)]
            Self::Unix(stream) => Pin::new(stream).poll_write(cx, buf),
        }

    }

    fn poll_write_vectored ( self: Pin<&mut Self>, cx: &mut Context<'_>, bufs: &[IoSlice<'_>] ) -> Poll<std::io::Result<usize>> {

        match self.get_mut() {
            Self::Tcp(stream) => Socket::gather(Pin::new(stream), cx, bufs),
            #[cfg(unix)]
            Self::Unix(stream) => Socket::gather(Pin::new(stream), cx, bufs),
        }

    }

    fn is_write_vectored ( &self ) -> bool {

        match self {
            Self::Tcp(stream) => stream.is_write_vectored(),
            #[cfg(unix)]
            Self::Unix(stream) => stream.is_write_vectored(),
        }

    }

    fn poll_flush ( self: Pin<&mut Self>, cx: &mut Context<'_> ) -> Poll<std::io::Result<()>> {

        match self.get_mut() {
            Self::Tcp(stream) => Pin::new(stream).poll_flush(cx),
            #[cfg(unix)]
            Self::Unix(stream) => Pin::new(stream).poll_flush(cx),
        }

    }

    fn poll_shutdown ( self: Pin<&mut Self>, cx: &mut Context<'_> ) -> Poll<std::io::Result<()>> {

        match self.get_mut() {
            Self::Tcp(stream) => Pin::new(stream).poll_shutdown(cx),
            #[cfg(unix)]
            Self::Unix(stream) => Pin::new(stream).poll_shutdown(cx),
        }

    }

}

impl Watched {

    pub fn new ( stream: Stream, since: Arc<AtomicU64> ) -> Self {

        Self { stream, stalled: false, since }

    }

    pub async fn prefaced ( &self ) -> bool {

        self.stream.prefaced().await

    }

    fn mark ( &mut self, blocked: bool ) {

        if blocked == self.stalled { return; }

        self.stalled = blocked;
        self.since.store(if blocked { Clock::stamp(Clock::recent()) } else { 0 }, Ordering::Relaxed);

    }

}

impl AsyncRead for Watched {

    fn poll_read ( self: Pin<&mut Self>, cx: &mut Context<'_>, buf: &mut ReadBuf<'_> ) -> Poll<std::io::Result<()>> {

        Pin::new(&mut self.get_mut().stream).poll_read(cx, buf)

    }

}

impl AsyncWrite for Watched {

    fn poll_write ( self: Pin<&mut Self>, cx: &mut Context<'_>, buf: &[u8] ) -> Poll<std::io::Result<usize>> {

        let this = self.get_mut();
        let outcome = Pin::new(&mut this.stream).poll_write(cx, buf);

        this.mark(outcome.is_pending());

        outcome

    }

    fn poll_write_vectored ( self: Pin<&mut Self>, cx: &mut Context<'_>, bufs: &[IoSlice<'_>] ) -> Poll<std::io::Result<usize>> {

        let this = self.get_mut();
        let outcome = Pin::new(&mut this.stream).poll_write_vectored(cx, bufs);

        this.mark(outcome.is_pending());

        outcome

    }

    fn is_write_vectored ( &self ) -> bool {

        self.stream.is_write_vectored()

    }

    fn poll_flush ( self: Pin<&mut Self>, cx: &mut Context<'_> ) -> Poll<std::io::Result<()>> {

        Pin::new(&mut self.get_mut().stream).poll_flush(cx)

    }

    fn poll_shutdown ( self: Pin<&mut Self>, cx: &mut Context<'_> ) -> Poll<std::io::Result<()>> {

        Pin::new(&mut self.get_mut().stream).poll_shutdown(cx)

    }

}
