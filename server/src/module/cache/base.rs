use std::sync::{Arc, atomic::{AtomicU64, Ordering}};
use std::time::{Duration};
use moka::sync::Cache;
use sha2::{Digest, Sha256};
use tokio::sync::Semaphore;

use crate::module::config::CacheConfig;
use super::{Caches, CachedResponse, Key};

impl Caches {

    pub fn new ( config: &CacheConfig ) -> Self {

        Self {
            response_generation: AtomicU64::new(0),
            scores: Cache::builder().max_capacity(config.max_entries).time_to_live(Duration::from_millis(config.score_ttl_ms)).build(),
            responses: Cache::builder().max_capacity(config.max_bytes)
                .weigher(|_: &Key, value: &Arc<CachedResponse>| value.weight)
                .time_to_live(Duration::from_millis(config.response_ttl_ms)).build(),
            fills: Arc::new(Semaphore::new(config.max_bytes as usize)),
            score_hits: AtomicU64::new(0), response_hits: AtomicU64::new(0),
        }

    }

    pub fn key ( parts: &[&[u8]] ) -> Key {

        let mut hash = Sha256::new();
        for part in parts { hash.update((part.len() as u64).to_le_bytes()); hash.update(part); }
        hash.finalize().into()

    }

    pub fn purge_responses ( &self ) {
        self.response_generation.fetch_add(1, Ordering::AcqRel);
        self.responses.invalidate_all();
    }

    pub fn stats ( &self ) -> serde_json::Value {

        serde_json::json!({
            "score_hits": self.score_hits.load(Ordering::Relaxed),
            "response_hits": self.response_hits.load(Ordering::Relaxed),
            "response_bytes": self.responses.weighted_size(),
        })

    }

}
