use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64};

use pingora::prelude::HttpPeer;

use crate::module::config::{BackendConfig, PoolConfig};

pub struct Backend {
    pub(super) available: Arc<Availability>,
    pub authority: http::HeaderValue,
    pub config: BackendConfig,
    pub peer: HttpPeer,
    pub(super) probe: Option<super::probe::Probe>,
    pub(super) active: AtomicU64,
    pub(super) latency_us: AtomicU64,
    pub(super) failures: AtomicU32,
    pub(super) down_until: AtomicU64,
    pub(super) alive: AtomicBool,
    pub next_probe: AtomicU64,
}

pub struct Pool {
    pub(super) available: Arc<Availability>,
    pub name: String,
    pub config: PoolConfig,
    pub backends: Vec<Arc<Backend>>,
    pub measure_latency: bool,
    pub(super) track_active: bool,
    pub(super) cursor: AtomicU64,
    pub(super) gate: Mutex<()>,
}

pub struct Lease {
    pub backend: Arc<Backend>,
    pub index: usize,
    pub(super) tracked: bool,
    pub(super) max_fails: u32,
    pub(super) cooldown_ms: u64,
}

#[derive(Default)]
pub(super) struct Availability { pub notify: tokio::sync::Notify, pub waiters: AtomicU64 }
