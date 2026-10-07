use http::header::HeaderValue;
use http::header::{CONNECTION, HeaderName, PROXY_AUTHENTICATE, PROXY_AUTHORIZATION, TE, TRAILER, TRANSFER_ENCODING, UPGRADE};

pub const X_REAL_IP: HeaderName        = HeaderName::from_static("x-real-ip");
pub const FORWARDED: HeaderName        = HeaderName::from_static("forwarded");

pub const KEEP_ALIVE: HeaderName       = HeaderName::from_static("keep-alive");
pub const PROXY_CONNECTION: HeaderName = HeaderName::from_static("proxy-connection");

pub const HOP_COUNT: usize = 9;

pub const HOP: [HeaderName; HOP_COUNT] = [
    CONNECTION, KEEP_ALIVE, PROXY_CONNECTION, PROXY_AUTHENTICATE, PROXY_AUTHORIZATION, TE, TRAILER, TRANSFER_ENCODING, UPGRADE,
];

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Header;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Var {
    RemoteAddr,
    RemotePort,
    Host,
    Scheme,
    RequestId,
    RequestUri,
    Uri,
    Args,
    UpstreamAddr,
    ServerPort,
    Msec,
    SslClientVerify,
    SslClientSDn,
    SslClientIDn,
    SslClientSerial,
    SslClientFingerprint,
    Derived(u16),
}

#[derive(Clone, Debug)]
pub enum Piece {
    Text(Box<[u8]>),
    Var(Var),
}

#[derive(Clone, Debug)]
pub struct Template {
    pub(super) pieces : Vec<Piece>,
}

#[derive(Clone, Debug)]
pub enum Rendered {
    Static(HeaderValue),
    Dynamic(Template),
    Append(HeaderValue),
    Default(HeaderValue),
    Remove,
}
