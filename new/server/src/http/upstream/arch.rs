use std::pin::Pin;
use std::time::Instant;

use bytes::Bytes;
use http::header::HeaderValue;
use http::uri::{Authority, Scheme};
use http_body::Frame;
use hyper::upgrade::OnUpgrade;
use hyper_util::client::legacy;
use hyper_util::rt::TokioIo;
use send_wrapper::SendWrapper;
use tokio::net::TcpStream;
use tokio::time::Sleep;
use tokio_rustls::client::TlsStream;

use crate::core::arena::Arena;
use crate::core::error::AppError;
use crate::core::net::Address;
use crate::core::sync::Local;
use crate::http::body::{Body, Guard, Incoming, Probe};
use crate::http::request::Req;
use crate::http::tls::Trust;

pub type Key = ( Address, bool );
pub type Wire = legacy::Client<Connector, Outbound>;

pub const PROBE_BYTES: usize = 65_536;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Protocol {
    #[default]
    Auto,
    Http1,
    Http2,
    Fastcgi,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Settings {
    pub connect_timeout_ms : u64,
    pub pool_idle_ms       : u64,
    pub pool_capacity      : usize,
    pub buffer             : usize,
    pub attempts           : usize,
}

#[derive(Clone, Debug)]
pub struct Upstream {
    pub key       : Key,
    pub slot      : usize,
    pub addr      : Address,
    pub authority : HeaderValue,
    pub scheme    : Scheme,
    pub origin    : Authority,
    pub trust     : Option<Trust>,
    pub protocol  : Protocol,
}

pub enum Io {
    Plain(TcpStream),
    Tls(Box<TlsStream<TcpStream>>),
    #[cfg(unix)]
    Unix(tokio::net::UnixStream),
}

pub struct Linked {
    pub(super) io : TokioIo<Io>,
    pub(super) h2 : bool,
}

#[derive(Clone)]
pub struct Connector {
    pub(super) addr       : Address,
    pub(super) trust      : Option<Trust>,
    pub(super) protocol   : Protocol,
    pub(super) timeout_ms : u64,
}

pub struct Outbound {
    pub(super) body : SendWrapper<Body>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Replay {
    pub failure : bool,
    pub status  : bool,
}

#[derive(Clone)]
pub struct Client {
    pub(super) wires    : Local<Vec<Option<Wire>>>,
    pub(super) arena    : Local<Arena>,
    pub(super) settings : Settings,
}

#[derive(Clone, Copy, Debug)]
pub struct Timing {
    pub started    : Instant,
    pub timeout_ms : u64,
}

pub struct Failure {
    pub error   : AppError,
    pub request : Option<Box<Req<Body>>>,
    pub connect : bool,
}

pub struct Streaming {
    pub(super) first    : Option<Frame<Bytes>>,
    pub(super) incoming : Incoming,
    pub(super) idle_ms  : u64,
    pub(super) timer    : Option<Pin<Box<Sleep>>>,
    pub(super) armed    : bool,
    pub(super) upgrade  : Option<OnUpgrade>,
    pub(super) guard    : Option<Guard>,
    pub(super) probe    : Option<Probe>,
}

pub enum Peeked {
    Whole(Option<Bytes>),
    Partial(Option<Frame<Bytes>>),
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Tunnel;
