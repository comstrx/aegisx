use std::net::SocketAddr;
use std::sync::Arc;

use serde::Deserialize;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Addr;

pub const GATHER_BYTES: usize = 4_096;
pub const TEXT: &[u8] = b"PROXY ";
pub const BINARY: &[u8] = b"\r\n\r\n\0\r\nQUIT\n";

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Socket;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Address {
    Tcp(SocketAddr),
    Unix(Arc<str>),
    Name(Arc<str>, u16),
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Nets {
    pub(super) v4 : Box<[( u32, u32 )]>,
    pub(super) v6 : Box<[( u128, u128 )]>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Preamble;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Wire {
    V1,
    V2,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Announced {
    Partial,
    Invalid,
    Done { length: usize, source: Option<SocketAddr> },
}
