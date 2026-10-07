use std::sync::Arc;
use std::sync::atomic::AtomicU64;

use bytes::Bytes;
use http::HeaderMap;
use http::header::HeaderName;
use tokio::sync::Semaphore;

use crate::core::cache::{Shelf, Weighted};
use crate::http::body::Probe;

pub struct Stored {
    pub status     : u16,
    pub headers    : HeaderMap,
    pub body       : Bytes,
    pub created_ms : u64,
    pub expires_ms : u64,
    pub refreshing : AtomicU64,
    pub vary       : Box<[HeaderName]>,
    pub epoch      : u64,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Ignore {
    pub control : bool,
    pub expires : bool,
    pub cookie  : bool,
    pub vary    : bool,
}

pub struct Store {
    pub(super) entries    : Weighted<Bytes, Arc<Stored>>,
    pub max_object        : usize,
    pub(super) valid      : Vec<( u16, u64 )>,
    pub(super) any        : Option<u64>,
    pub(super) stale_ms   : u64,
    pub(super) vary       : Vec<HeaderName>,
    pub rescue            : bool,
    pub(super) lock       : bool,
    pub(super) ignore     : Ignore,
    pub(super) fills      : papaya::HashMap<Bytes, Arc<Semaphore>>,
    pub lock_ms           : u64,
    pub(super) shelf      : Option<Arc<Shelf>>,
    pub(super) epoch      : AtomicU64,
    pub(super) hits       : AtomicU64,
    pub(super) misses     : AtomicU64,
    pub(super) stale      : AtomicU64,
    pub(super) filled     : AtomicU64,
    pub(super) bans       : std::sync::RwLock<Vec<Ban>>,
    pub(super) banned     : AtomicU64,
}

#[derive(Clone, Debug, Default)]
pub struct Ban {
    pub host   : Option<Box<str>>,
    pub prefix : Option<Box<str>>,
    pub tag    : Option<Box<str>>,
    pub at_ms  : u64,
}

pub enum Lookup {
    Hit(Arc<Stored>),
    Stale(Arc<Stored>),
    Revalidate(Arc<Stored>),
    Refresh(Arc<Stored>),
    Pass,
    Miss,
}

pub struct Claim {
    pub(super) store  : Arc<Store>,
    pub(super) key    : Bytes,
    pub(super) gate   : Arc<Semaphore>,
    pub(super) stored : bool,
}

pub struct Fill {
    pub(super) store      : Arc<Store>,
    pub(super) key        : Bytes,
    pub(super) status     : u16,
    pub(super) headers    : HeaderMap,
    pub(super) ttl_ms     : u64,
    pub(super) created_ms : u64,
    pub(super) tap        : Probe,
    pub(super) claim      : Option<Claim>,
}
