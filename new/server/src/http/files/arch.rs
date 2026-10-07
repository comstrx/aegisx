use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;

use bytes::Bytes;
use http::header::HeaderValue;

use crate::core::cache::Weighted;

#[derive(Debug)]
pub struct Files {
    pub(super) root          : PathBuf,
    pub(super) index         : Option<String>,
    pub(super) cache_control : Option<HeaderValue>,
    pub(super) autoindex     : bool,
}

pub struct Loaded {
    pub(super) bytes    : Bytes,
    pub(super) length   : u64,
    pub(super) modified : Option<u64>,
    pub(super) etag     : HeaderValue,
    pub(super) kind     : &'static str,
    pub(super) checked  : AtomicU64,
    pub(super) packed   : Vec<Packed>,
}

pub struct Packed {
    pub(super) coding : HeaderValue,
    pub(super) bytes  : Bytes,
    pub(super) etag   : HeaderValue,
}

pub const PACKED: [( &str, &str ); 3] = [( "br", "br" ), ( "zst", "zstd" ), ( "gz", "gzip" )];

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Key {
    File(PathBuf),
    Dir(PathBuf),
}

pub struct FileCache {
    pub(super) store          : Weighted<Key, Arc<Loaded>>,
    pub(super) max_file_bytes : u64,
    pub(super) valid_ms       : u64,
    pub(super) precompressed  : bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Candidate {
    Uri,
    Dir,
    Path(Box<str>),
    Status(u16),
    Upstream,
}

#[derive(Clone, Copy)]
pub struct Fetch <'a> {
    pub method  : &'a http::Method,
    pub headers : &'a http::HeaderMap,
    pub path    : &'a str,
    pub strip   : usize,
    pub query   : Option<&'a str>,
    pub now_ms  : u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Range {
    Full,
    Part(u64, u64),
    Many(Vec<( u64, u64 )>),
    Unsatisfiable,
}
