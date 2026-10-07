use std::sync::Arc;
use std::time::Instant;
use std::sync::atomic::AtomicU64;
use bytes::Bytes;
use moka::sync::Cache;
use pingora::http::ResponseHeader;
use tokio::sync::{OwnedSemaphorePermit, Semaphore};

pub type Key = [u8; 32];

pub struct CachedResponse {
    pub header: ResponseHeader,
    pub body: Bytes,
    pub created: Instant,
    pub expires: Instant,
    pub initial_age: u64,
    pub weight: u32,
}

pub struct Fill {
    pub key: Key,
    pub header: ResponseHeader,
    pub bytes: Vec<u8>,
    pub limit: usize,
    pub created: Instant,
    pub expires: Instant,
    pub initial_age: u64,
    pub(super) _permit: OwnedSemaphorePermit,
}

pub struct Caches {
    pub response_generation: AtomicU64,
    pub scores: Cache<Key, crate::core::domain::RiskScores>,
    pub responses: Cache<Key, Arc<CachedResponse>>,
    pub fills: Arc<Semaphore>,
    pub score_hits: AtomicU64,
    pub response_hits: AtomicU64,
}
