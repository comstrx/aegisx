use bytes::BytesMut;
use bytes::buf::Writer;
use flate2::write::GzEncoder;
use http::HeaderMap;

use crate::http::body::{Body, Guard, Probe};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Encoding {
    Gzip,
    Brotli,
    Zstd,
}

#[derive(Debug)]
pub struct Compression {
    pub(super) min_bytes    : u64,
    pub(super) level        : u32,
    pub(super) brotli       : bool,
    pub(super) brotli_level : u32,
    pub(super) zstd         : bool,
    pub(super) zstd_level   : i32,
    pub(super) types        : Vec<Box<str>>,
}

pub(super) enum Codec {
    Gzip(Box<GzEncoder<Writer<BytesMut>>>),
    Brotli(Box<brotli::CompressorWriter<Writer<BytesMut>>>),
    Zstd(Box<zstd::stream::write::Encoder<'static, Writer<BytesMut>>>),
}

pub struct Decoded {
    pub(super) inner    : Body,
    pub(super) decoder  : Option<Box<flate2::write::GzDecoder<Writer<BytesMut>>>>,
    pub(super) trailers : Option<HeaderMap>,
    pub(super) probe    : Option<Probe>,
    pub(super) guard    : Option<Guard>,
}

#[derive(Clone, Debug)]
pub struct Swaps {
    pub rules : std::sync::Arc<[( bytes::Bytes, bytes::Bytes )]>,
    pub types : Box<[Box<str>]>,
}

pub struct Replaced {
    pub(super) inner    : Body,
    pub(super) rules    : std::sync::Arc<[( bytes::Bytes, bytes::Bytes )]>,
    pub(super) carry    : BytesMut,
    pub(super) longest  : usize,
    pub(super) ended    : bool,
    pub(super) trailers : Option<HeaderMap>,
    pub(super) probe    : Option<Probe>,
    pub(super) guard    : Option<Guard>,
}

pub struct Encoded {
    pub(super) inner    : Body,
    pub(super) codec    : Option<Codec>,
    pub(super) tail     : BytesMut,
    pub(super) dirty    : bool,
    pub(super) trailers : Option<HeaderMap>,
    pub(super) probe    : Option<Probe>,
    pub(super) guard    : Option<Guard>,
}
