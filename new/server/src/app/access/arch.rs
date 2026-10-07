use std::cell::RefCell;
use std::net::SocketAddr;
use std::rc::Rc;
use std::sync::atomic::AtomicU64;
use std::sync::mpsc::SyncSender;
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Instant;

use http::header::HeaderName;

use crate::config::AccessFormat;
use crate::core::list::Few;
use crate::core::net::Address;
use crate::http::body::Probe;

pub struct Entry {
    pub peer       : SocketAddr,
    pub secure     : bool,
    pub captured   : Vec<u8>,
    pub cuts       : Few<u16, 8>,
    pub method     : http::Method,
    pub version    : http::Version,
    pub status     : u16,
    pub text       : Vec<u8>,
    pub marks      : [u16; 4],
    pub route      : Option<Arc<str>>,
    pub backend    : Option<Address>,
    pub header_us  : u64,
    pub attempts   : u8,
    pub sent       : Option<Probe>,
    pub received   : u64,
    pub started    : Instant,
    pub started_ms : u64,
}

pub struct Access {
    pub(super) pattern : Option<Arc<Pattern>>,
    pub(super) sender  : Mutex<Option<SyncSender<Vec<u8>>>>,
    pub(super) thread  : Mutex<Option<JoinHandle<()>>>,
    pub(super) dropped : AtomicU64,
}

pub struct Log {
    pub(super) access   : Arc<Access>,
    pub(super) sender   : SyncSender<Vec<u8>>,
    pub(super) format   : AccessFormat,
    pub(super) pattern  : Option<Arc<Pattern>>,
    pub(super) limit    : usize,
    pub(super) flush_ms : u64,
    pub(super) floor    : u16,
    pub(super) buffer   : RefCell<Vec<u8>>,
    pub(super) stamp    : RefCell<( u64, [u8; 26] )>,
}

pub type Journal = Rc<Log>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Field {
    RemoteAddr,
    RemotePort,
    TimeLocal,
    TimeIso8601,
    Msec,
    Request,
    RequestMethod,
    RequestUri,
    Uri,
    Args,
    ServerProtocol,
    Status,
    BodyBytesSent,
    RequestLength,
    HttpReferer,
    HttpUserAgent,
    RequestTime,
    RequestTimeUs,
    UpstreamAddr,
    UpstreamResponseTime,
    UpstreamStatus,
    UpstreamAttempts,
    Route,
    RequestId,
    Scheme,
    Capture(usize),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Capture {
    Header(HeaderName),
    User,
    ClientDn,
}

#[derive(Debug)]
pub enum Piece {
    Text(Box<[u8]>),
    Field(Field),
}

#[derive(Debug)]
pub struct Pattern {
    pub(super) pieces   : Vec<Piece>,
    pub(super) captures : Vec<Capture>,
}

pub enum Sink {
    Stdout,
    File(std::path::PathBuf),
    Rotating(Box<dyn std::io::Write>),
    Datagram { target: String, socket: Option<std::net::UdpSocket>, syslog: bool },
    Stream { target: String, link: Option<std::net::TcpStream> },
}
