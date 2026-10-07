use std::collections::{HashMap, hash_map::RandomState};
use std::net::IpAddr;
use std::sync::Mutex;
use std::sync::atomic::AtomicU64;

#[derive(Clone, Copy, Default)]
pub(super) struct Bucket {
    pub second: u64,
    pub requests: u64,
    pub failures: u64,
    pub blocks: u64,
}

pub(super) struct Entry {
    pub first_ms: u64,
    pub requests_seen: u64,
    pub last_request_ms: u64,
    pub last_ms: u64,
    pub in_flight: u64,
    pub latency_ms: u64,
    pub last_completion_ms: u64,
    pub buckets: [Bucket; 10],
    pub totals: Bucket,
    pub latest_second: u64,
}

pub struct Memory<K = IpAddr> {
    pub(super) shards: Vec<Shard<K>>,
    pub(super) hasher: RandomState,
    pub(super) idle_ttl_ms: u64,
}

pub(super) struct Shard<K> {
    pub(super) entries: Mutex<HashMap<K, Entry>>,
    pub(super) capacity: usize,
    pub(super) next_cleanup_ms: AtomicU64,
}

#[derive(Clone, Copy, Default)]
pub struct Snapshot {
    pub requests: u64,
    pub failures: u64,
    pub blocks: u64,
    pub in_flight: u64,
    pub gap_seconds: f32,
    pub age_seconds: f32,
    pub latency_ms: u64,
}
