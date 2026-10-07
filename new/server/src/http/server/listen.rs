use std::net::SocketAddr;

use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc::channel;

use crate::core::error::{AppError, AppFail, AppResult};
use crate::core::net::Socket;
use super::arch::{Accept, Listener, Source, Stream, Transport};

const FANOUT_DEPTH: usize = 1_024;

impl Accept {

    pub fn resolve ( self ) -> Self {

        match self {
            Self::Auto if cfg!(target_os = "linux") => Self::ReusePort,
            Self::Auto => Self::Shared,
            other => other,
        }

    }

}

impl Transport for TcpStream {

    type Std = std::net::TcpStream;
    type Listener = TcpListener;

    fn into_std ( self ) -> std::io::Result<Self::Std> { TcpStream::into_std(self) }

    fn from_std ( std: Self::Std ) -> std::io::Result<Self> { TcpStream::from_std(std) }

    async fn accept ( listener: &Self::Listener ) -> std::io::Result<( Self, SocketAddr )> { listener.accept().await }

    fn stream ( self ) -> Stream { Stream::Tcp(self) }

}

#[cfg(unix)]
impl Transport for tokio::net::UnixStream {

    type Std = std::os::unix::net::UnixStream;
    type Listener = tokio::net::UnixListener;

    fn into_std ( self ) -> std::io::Result<Self::Std> { tokio::net::UnixStream::into_std(self) }

    fn from_std ( std: Self::Std ) -> std::io::Result<Self> { tokio::net::UnixStream::from_std(std) }

    async fn accept ( listener: &Self::Listener ) -> std::io::Result<( Self, SocketAddr )> { listener.accept().await.map(|( stream, _ )| ( stream, SocketAddr::from(( [0, 0, 0, 0], 0 )) )) }

    fn stream ( self ) -> Stream { Stream::Unix(self) }

}

impl Listener {

    pub fn bind ( addr: SocketAddr, backlog: i32, count: usize, accept: Accept ) -> AppResult<Vec<Self>> {

        let count = count.max(1);

        let sources = match accept.resolve() {
            Accept::ReusePort if count > 1 => (0..count).map(|_| Ok(Source::Socket(Some(Socket::listen(addr, backlog, true)?), None))).collect::<AppResult<Vec<_>>>()?,
            _ if count == 1 => vec![Source::Socket(Some(Socket::listen(addr, backlog, false)?), None)],
            _ => Self::fanout(Socket::listen(addr, backlog, false)?, count),
        };

        Ok(sources.into_iter().map(|tcp| Self { tcp, unix: None }).collect())

    }

    #[cfg(unix)]
    pub fn bind_unix ( listeners: &mut [Self], path: &str, backlog: i32 ) -> AppResult<()> {

        let count = listeners.len().max(1);
        let mut sources = if count == 1 { vec![Source::Socket(Some(Socket::listen_unix(path, backlog)?), None)] } else { Self::fanout(Socket::listen_unix(path, backlog)?, count) };

        for ( listener, source ) in listeners.iter_mut().zip(sources.drain(..)) { listener.unix = Some(source); }

        Ok(())

    }

    fn fanout <S: Transport> ( socket: <S::Listener as super::arch::Listening>::Std, count: usize ) -> Vec<Source<S>> {

        let mut sources = Vec::with_capacity(count);
        let mut senders = Vec::with_capacity(count - 1);

        for _ in 1..count {

            let ( sender, receiver ) = channel::<( S::Std, SocketAddr )>(FANOUT_DEPTH);

            senders.push(sender);
            sources.push(Source::Channel(receiver));

        }

        sources.insert(0, Source::Fanout { pending: Some(socket), socket: None, senders, next: 0 });

        sources

    }

    pub async fn accept ( &mut self ) -> Option<AppResult<( Stream, SocketAddr )>> {

        match &mut self.unix {
            None => Self::take(&mut self.tcp).await.map(|outcome| outcome.map(|( stream, peer )| ( stream.stream(), peer ))),
            Some(unix) => tokio::select! {
                accepted = Self::take(&mut self.tcp) => accepted.map(|outcome| outcome.map(|( stream, peer )| ( stream.stream(), peer ))),
                accepted = Self::take(unix) => accepted.map(|outcome| outcome.map(|( stream, peer )| ( stream.stream(), peer ))),
            },
        }

    }

    async fn take <S: Transport> ( source: &mut Source<S> ) -> Option<AppResult<( S, SocketAddr )>> {

        match source {
            Source::Socket(pending, socket) => {

                let socket = match Self::ready::<S>(pending, socket) { Ok(socket) => socket, Err(error) => return Some(Err(error)) };

                Some(S::accept(socket).await.or_fail("accept failed"))

            }
            Source::Channel(receiver) => {

                let ( stream, peer ) = receiver.recv().await?;

                Some(S::from_std(stream).map(|stream| ( stream, peer )).or_fail("cannot register accepted socket"))

            }
            Source::Fanout { pending, socket, senders, next } => {

                let socket = match Self::ready::<S>(pending, socket) { Ok(socket) => socket, Err(error) => return Some(Err(error)) };

                loop {

                    let ( stream, peer ) = match S::accept(socket).await { Ok(accepted) => accepted, Err(error) => return Some(Err(AppError::network("accept", error.to_string()))) };

                    if senders.is_empty() { return Some(Ok(( stream, peer ))); }

                    *next = (*next + 1) % (senders.len() + 1);

                    let Some(target) = next.checked_sub(1).and_then(|index| senders.get(index)) else { return Some(Ok(( stream, peer ))); };

                    let Ok(std) = stream.into_std() else { continue; };

                    match target.try_send(( std, peer )) {
                        Ok(()) => continue,
                        Err(rejected) => {

                            let ( std, peer ) = rejected.into_inner();

                            return Some(S::from_std(std).map(|stream| ( stream, peer )).or_fail("cannot register accepted socket"));

                        }
                    }

                }

            }
        }

    }

    fn ready <'s, S: Transport> ( pending: &mut Option<<S::Listener as super::arch::Listening>::Std>, socket: &'s mut Option<S::Listener> ) -> AppResult<&'s mut S::Listener> {

        if socket.is_none() {

            let listener = pending.take().ok_or_else(|| AppError::message("listener already consumed"))?;

            *socket = Some(<S::Listener as super::arch::Listening>::register(listener).or_fail("cannot register listener")?);

        }

        socket.as_mut().ok_or_else(|| AppError::message("listener unavailable"))

    }

}

impl super::arch::Listening for TcpListener {

    type Std = std::net::TcpListener;

    fn register ( std: Self::Std ) -> std::io::Result<Self> { TcpListener::from_std(std) }

}

#[cfg(unix)]
impl super::arch::Listening for tokio::net::UnixListener {

    type Std = std::os::unix::net::UnixListener;

    fn register ( std: Self::Std ) -> std::io::Result<Self> { tokio::net::UnixListener::from_std(std) }

}
