use std::collections::VecDeque;
use std::any::Any;
use std::cell::RefCell;
use std::pin::Pin;
use std::rc::Rc;

use std::time::Instant;

use bytes::{Bytes, BytesMut};
use tokio::time::Sleep;

use crate::http::encode::{Decoded, Encoded, Replaced};
use crate::http::upstream::Streaming;

pub type Incoming = hyper::body::Incoming;

pub type Guard = Rc<dyn Any>;

pub type Probe = Rc<RefCell<Tap>>;

pub struct Tap {
    pub(super) buffer : Vec<u8>,
    pub(super) cap    : usize,
    pub(super) seen   : usize,
    pub(super) done   : bool,
}

pub struct Chained {
    pub(super) head : Option<Bytes>,
    pub(super) rest : Body,
}

pub enum Inner {
    Hyper(Incoming),
    Quic(Box<QuicBody>),
}

pub struct QuicBody {
    pub(super) stream : h3::server::RequestStream<h3_quinn::RecvStream, Bytes>,
    pub(super) data   : bool,
    pub(super) probe  : Option<Probe>,
}

pub struct Limited {
    pub(super) inner     : Inner,
    pub(super) remaining : usize,
    pub(super) idle_ms   : u64,
    pub(super) timer     : Option<Pin<Box<Sleep>>>,
    pub(super) armed     : bool,
    pub(super) probe     : Option<Probe>,
}

pub struct FileBody {
    pub(super) file      : tokio::fs::File,
    pub(super) remaining : u64,
    pub(super) buffer    : BytesMut,
    pub(super) probe     : Option<Probe>,
    pub(super) guard     : Option<Guard>,
}

pub struct Paced {
    pub(super) inner   : Body,
    pub(super) rate    : u64,
    pub(super) free    : u64,
    pub(super) sent    : u64,
    pub(super) started : Option<Instant>,
    pub(super) held    : Option<Bytes>,
    pub(super) timer   : Option<Pin<Box<Sleep>>>,
}

pub enum Body {
    Incoming(Incoming),
    Quic(Box<QuicBody>),
    Upstream(Box<Streaming>),
    Limited(Limited),
    File(Box<FileBody>),
    Encoded(Box<Encoded>),
    Decoded(Box<Decoded>),
    Replaced(Box<Replaced>),
    Boxed(Box<dyn Frames>),
    Paced(Box<Paced>),
    Chained(Box<Chained>),
    Chunks(VecDeque<Bytes>),
    Bytes(Bytes),
    Empty,
}

pub trait Frames {
    fn frame ( &mut self, cx: &mut std::task::Context<'_> ) -> std::task::Poll<Option<Result<http_body::Frame<Bytes>, crate::core::error::AppError>>>;
    fn ended ( &self ) -> bool;
    fn guard ( &mut self, guard: Guard );
    fn probe ( &mut self, probe: Probe );
}
