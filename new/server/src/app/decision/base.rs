use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use aws_lc_rs::digest::{SHA256, digest};
use serde_json::{Value, json};
use tokio::sync::oneshot;

use crate::app::{Key, Memory};
use crate::config::DecisionConfig;
use crate::core::cache::Cache;
use crate::core::db::Db;
use crate::core::error::{AppError, AppResult};
use crate::core::log::{info, warn};
use crate::core::queue::{Queue, Spec};
use crate::core::time::Clock;
use super::arch::{Decisions, Verdict, Write};

impl Decisions {

    pub fn open ( config: &DecisionConfig ) -> AppResult<Self> {

        let db = Arc::new(Db::open(&config.path)?);

        Self::schema(&db)?;

        let cache = Arc::new(Cache::new(config.capacity, config.deny_ttl_ms));
        let loaded = Self::load(&db, &cache)?;
        let write_failures = Self::counter();

        let writer = {

            let db = db.clone();
            let cache = cache.clone();
            let failures = write_failures.clone();

            Queue::start(Spec { name: "aegisx-decisions", workers: 1, capacity: 1_024, deadline_ms: 10_000 }, move |mut write: Write, _| {

                let outcome = Self::persist(&db, &cache, &write);

                if outcome.is_err() { Self::bump(&failures); }

                if let Some(done) = write.done.take() { let _ = done.send(outcome.as_ref().map(|_| ()).map_err(|error| AppError::message(error.to_string()))); }

                outcome

            })?

        };

        info!(path = %config.path.display(), loaded, "decision store ready");

        Ok(Self { db, cache, writer, deny_ttl_ms: config.deny_ttl_ms, hits: AtomicU64::new(0), misses: AtomicU64::new(0), write_failures, generation: AtomicU64::new(0) })

    }

    pub fn key ( route: &str, actor: &Key ) -> Key {

        let mut material = Vec::with_capacity(route.len() + 42);

        material.extend_from_slice(b"decision:");
        material.extend_from_slice(route.as_bytes());
        material.push(b':');
        material.extend_from_slice(actor);

        let mut key = [0u8; 32];

        key.copy_from_slice(digest(&SHA256, &material).as_ref());

        key

    }

    pub fn lookup ( &self, key: &Key ) -> Option<Arc<Verdict>> {

        let found = self.cache.get(key);

        match found.is_some() {
            true => self.hits.fetch_add(1, Ordering::Relaxed),
            false => self.misses.fetch_add(1, Ordering::Relaxed),
        };

        found

    }

    pub fn deny_ttl_ms ( &self ) -> u64 {

        self.deny_ttl_ms

    }

    pub fn verdict ( &self, route: &str, actor: &Key, ttl_ms: u64, reason: &str, source: &str, request_id: Option<String> ) -> ( Key, Verdict ) {

        let key = Self::key(route, actor);
        let now = Clock::now_ms();

        let verdict = Verdict {
            key        : Memory::hex(&key),
            actor      : Memory::hex(actor),
            route      : route.to_string(),
            reason     : reason.to_string(),
            source     : source.to_string(),
            created_ms : now,
            expires_ms : now + ttl_ms.clamp(1, self.deny_ttl_ms),
            request_id,
        };

        ( key, verdict )

    }

    pub fn block ( &self, key: Key, verdict: Verdict ) -> oneshot::Receiver<AppResult<()>> {

        self.submit(Write { key, verdict: Some(Arc::new(verdict)), done: None })

    }

    pub fn revoke ( &self, key: Key ) -> oneshot::Receiver<AppResult<()>> {

        self.submit(Write { key, verdict: None, done: None })

    }

    pub fn purge ( &self ) -> AppResult<usize> {

        let loaded = Self::load(&self.db, &self.cache)?;

        self.generation.fetch_add(1, Ordering::Relaxed);

        Ok(loaded)

    }

    pub fn state ( &self ) -> Value {

        json!({
            "hits"           : self.hits.load(Ordering::Relaxed),
            "misses"         : self.misses.load(Ordering::Relaxed),
            "write_failures" : self.write_failures.load(Ordering::Relaxed),
            "cached_keys"    : self.cache.len(),
            "queued"         : self.writer.queued(),
            "generation"     : self.generation.load(Ordering::Relaxed),
        })

    }

    pub fn stop ( &self ) {

        self.writer.stop();

        if let Err(error) = self.db.checkpoint() { warn!(%error, "decision database checkpoint failed"); }

    }

    fn submit ( &self, mut write: Write ) -> oneshot::Receiver<AppResult<()>> {

        let ( sender, receiver ) = oneshot::channel();

        write.done = Some(sender);

        if let Err(rejected) = self.writer.submit(write) {

            Self::bump(&self.write_failures);

            if let Some(done) = rejected.done { let _ = done.send(Err(AppError::unsupported("decision writer is saturated"))); }

        }

        receiver

    }

}
