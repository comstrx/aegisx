use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64};
use std::time::Instant;

use crate::config::{Balance, HealthConfig};
use crate::core::net::Address;
use crate::core::sync::Tally;
use crate::http::upstream::Upstream;

pub use crate::http::key::{HashKey, Hint};

pub struct Backend {
    pub index      : usize,
    pub id         : Box<str>,
    pub addr       : Address,
    pub origin     : Option<Arc<str>>,
    pub upstream   : Upstream,
    pub weight     : u32,
    pub limit      : u64,
    pub backup     : bool,
    pub down       : bool,
    pub active     : AtomicU64,
    pub fails      : AtomicU32,
    pub down_until : AtomicU64,
    pub revived    : AtomicU64,
    pub probed     : AtomicBool,
    pub latency_us : AtomicU64,
    pub counts     : Arc<[Counts]>,
}

#[repr(align(128))]
#[derive(Default)]
pub struct Counts {
    pub served  : Tally,
    pub failed  : Tally,
    pub retried : Tally,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Retry {
    pub connect        : bool,
    pub error          : bool,
    pub timeout        : bool,
    pub statuses       : Vec<u16>,
    pub server_errors  : bool,
    pub non_idempotent : bool,
}

pub struct PoolState {
    pub index       : usize,
    pub name        : Arc<str>,
    pub backends    : Vec<Arc<Backend>>,
    pub policy      : Balance,
    pub max_fails   : u32,
    pub cooldown_ms : u64,
    pub slow_start  : u64,
    pub max_ejected : u32,
    pub attempts    : usize,
    pub keepalive   : usize,
    pub slow_us     : u64,
    pub retry       : Retry,
    pub counting    : bool,
    pub health      : Option<HealthConfig>,
    pub expect      : Option<regex::bytes::Regex>,
    pub hash        : Option<HashKey>,
    pub sticky      : Option<Sticky>,
    pub spare       : bool,
}

#[derive(Clone, Debug)]
pub struct Sticky {
    pub cookie : Box<str>,
    pub suffix : Box<str>,
}

pub struct Pools {
    pub list    : Vec<Arc<PoolState>>,
    pub started : Instant,
}

pub type Resolved = std::collections::HashMap<Arc<str>, Vec<std::net::SocketAddr>>;

pub struct Picker {
    pub(super) cursors : Vec<usize>,
    pub(super) current : Vec<Vec<i64>>,
    pub(super) latency : Vec<Vec<u64>>,
    pub(super) samples : Vec<Vec<u32>>,
    pub(super) seed    : u64,
}

pub struct Lease {
    pub backend : Arc<Backend>,
}
