use std::net::SocketAddr;

use http::header::{HeaderName, HeaderValue};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Identity;

#[derive(Clone, Debug)]
pub struct Certificate {
    pub subject     : Box<str>,
    pub issuer      : Box<str>,
    pub serial      : Box<str>,
    pub fingerprint : Box<str>,
}

pub struct Peer {
    pub addr    : SocketAddr,
    pub trusted : bool,
    pub forward : HeaderValue,
    pub proto   : HeaderValue,
}

#[derive(Clone, Debug)]
pub struct Names {
    pub request_id      : HeaderName,
    pub forwarded_for   : HeaderName,
    pub forwarded_proto : HeaderName,
    pub actor           : Option<HeaderName>,
    pub block           : Option<HeaderName>,
    pub drop_trusted    : Vec<HeaderName>,
    pub drop_untrusted  : Vec<HeaderName>,
}
