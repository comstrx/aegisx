use std::cell::{Cell, RefCell};
use std::future::Future;
use std::net::SocketAddr;
use std::pin::Pin;
use std::rc::{Rc, Weak};
use std::sync::Arc;
use std::sync::atomic::AtomicU64;
use std::task::Waker;
use std::time::Instant;

use serde::Deserialize;
use tokio::net::TcpStream;
use tokio::task::AbortHandle;
use tokio::sync::mpsc::{Receiver, Sender};

use crate::core::sync::{Local, Swap};
use crate::http::tls::Acceptor;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Accept {
    #[default]
    Auto,
    ReusePort,
    Shared,
}

pub trait Listening: Sized {
    type Std: Send + 'static;
    fn register ( std: Self::Std ) -> std::io::Result<Self>;
}

pub trait Transport: Sized + Unpin + 'static {
    type Std: Send + 'static;
    type Listener: Listening + 'static;
    fn into_std ( self ) -> std::io::Result<Self::Std>;
    fn from_std ( std: Self::Std ) -> std::io::Result<Self>;
    fn accept ( listener: &Self::Listener ) -> impl Future<Output = std::io::Result<( Self, SocketAddr )>>;
    fn stream ( self ) -> Stream;
}

pub enum Source <S: Transport> {
    Socket(Option<<S::Listener as Listening>::Std>, Option<S::Listener>),
    Channel(Receiver<( S::Std, SocketAddr )>),
    Fanout { pending: Option<<S::Listener as Listening>::Std>, socket: Option<S::Listener>, senders: Vec<Sender<( S::Std, SocketAddr )>>, next: usize },
}

pub enum Stream {
    Tcp(TcpStream),
    #[cfg(unix)]
    Unix(tokio::net::UnixStream),
}

#[cfg(unix)]
pub type UnixSocketStream = tokio::net::UnixStream;

#[cfg(not(unix))]
pub type UnixSocketStream = TcpStream;

pub type Sessions <C> = Rc<RefCell<Vec<( Weak<C>, AbortHandle, Arc<AtomicU64>, Rc<Cell<Option<Waker>>> )>>>;

pub struct Slot {
    pub(super) active : Local<usize>,
}

pub struct Graceful <F> {
    pub(super) inner  : Pin<Box<F>>,
    pub(super) close  : fn(Pin<&mut F>),
    pub(super) stage  : Rc<Cell<u8>>,
    pub(super) waker  : Rc<Cell<Option<Waker>>>,
    pub(super) after  : u8,
    pub(super) armed  : bool,
    pub(super) closed : bool,
}

pub struct Watched {
    pub(super) stream  : Stream,
    pub(super) stalled : bool,
    pub(super) since   : Arc<AtomicU64>,
}

pub const HYPER_MAX_HEADERS: usize = 100;
pub const DRAINING: u8 = 1;
pub const CLOSING: u8 = 2;
pub const GRACE_MS: u64 = 1_000;
pub const FLUSH_MAX: usize = 4_096;
pub const PREFACE: &[u8] = b"PRI * HTTP/2.0\r\n\r\nSM\r\n\r\n";
pub const PREAMBLE_BYTES: usize = 2_048;
pub const HELLO_BYTES: usize = 16_384;

pub struct Listener {
    pub(super) tcp  : Source<TcpStream>,
    pub(super) unix : Option<Source<UnixSocketStream>>,
}

pub trait Session {
    fn idle ( &self ) -> Option<( Instant, bool )>;
    fn identify ( &self, _certificate: &[u8] ) {}
}

impl Session for SocketAddr {

    fn idle ( &self ) -> Option<( Instant, bool )> {

        None

    }

}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Settings {
    pub keepalive            : bool,
    pub max_headers          : usize,
    pub max_connections      : usize,
    pub header_timeout_ms    : u64,
    pub keepalive_timeout_ms : u64,
    pub send_timeout_ms      : u64,
    pub buffer               : usize,
    pub drain_ms             : u64,
    pub http2                : bool,
    pub h2c                  : bool,
    pub max_streams          : u32,
    pub h2_adaptive_window   : bool,
    pub h2_stream_window     : u32,
    pub h2_connection_window : u32,
    pub h2_max_frame         : u32,
    pub h2_max_header_bytes  : u32,
    pub proxy_protocol       : bool,
}

pub struct Server {
    pub(super) settings : Settings,
    pub(super) tls      : Swap<Option<Acceptor>>,
}
