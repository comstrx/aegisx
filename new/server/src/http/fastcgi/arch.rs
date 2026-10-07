use std::cell::Cell;
use std::net::SocketAddr;
use std::pin::Pin;
use std::rc::Rc;
use std::time::Instant;

use bytes::Bytes;
use tokio::io::BufWriter;
use tokio_io_timeout::TimeoutStream;

use crate::core::error::AppError;
use crate::core::sync::Local;
use crate::http::body::{Guard, Probe};
use crate::http::server::Stream;

pub type Link = Pin<Box<TimeoutStream<BufWriter<Stream>>>>;

pub type Chunks = Pin<Box<dyn futures_util::Stream<Item = Result<Bytes, AppError>>>>;

#[derive(Clone)]
pub struct Fcgi {
    pub(super) idle     : Local<Vec<Vec<( Link, Instant )>>>,
    pub(super) idle_ms  : u64,
    pub(super) sweeping : Rc<Cell<bool>>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Script {
    pub root   : Box<str>,
    pub index  : Box<str>,
    pub suffix : Box<str>,
}

#[derive(Clone, Copy, Debug)]
pub struct Call <'a> {
    pub script     : &'a Script,
    pub peer       : SocketAddr,
    pub port       : u16,
    pub secure     : bool,
    pub timeout_ms : u64,
    pub keep       : usize,
}

pub struct Output {
    pub(super) chunks : Chunks,
    pub(super) rest   : Option<Bytes>,
    pub(super) done   : bool,
    pub(super) probe  : Option<Probe>,
    pub(super) guard  : Option<Guard>,
}
