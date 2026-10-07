use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use rusqlite::params;

use crate::app::{Key, Memory};
use crate::core::cache::Cache;
use crate::core::db::Db;
use crate::core::error::{AppError, AppResult};
use crate::core::parse::Json;
use crate::core::time::Clock;
use super::arch::{Decisions, Verdict, Write};

const SCHEMA: &str = "
    CREATE TABLE IF NOT EXISTS verdicts (key TEXT PRIMARY KEY, expires_ms INTEGER NOT NULL, payload TEXT NOT NULL);
    CREATE INDEX IF NOT EXISTS verdicts_expiry ON verdicts (expires_ms);
    CREATE TABLE IF NOT EXISTS private_settings (key TEXT PRIMARY KEY, value BLOB NOT NULL);
";

impl Decisions {

    pub(super) fn schema ( db: &Db ) -> AppResult<()> {

        db.batch(SCHEMA)

    }

    pub fn identity_key ( &self ) -> AppResult<[u8; 32]> {

        if let Some(value) = self.db.first("SELECT value FROM private_settings WHERE key = 'identity_key'", [], |row| row.get::<_, Vec<u8>>(0))? {

            return value.try_into().map_err(|_| AppError::invalid("identity key", "stored key is not 32 bytes"));

        }

        let mut key = [0u8; 32];

        getrandom::fill(&mut key).map_err(|error| AppError::invalid("identity key", error.to_string()))?;
        self.db.execute("INSERT INTO private_settings (key, value) VALUES ('identity_key', ?1)", params![key.to_vec()])?;

        Ok(key)

    }

    pub(super) fn persist ( db: &Db, cache: &Cache<Key, Arc<Verdict>>, write: &Write ) -> AppResult<()> {

        let hex = Memory::hex(&write.key);

        match &write.verdict {
            Some(verdict) => {

                let payload = Json::to_string(verdict.as_ref())?;
                let remaining = verdict.expires_ms.saturating_sub(Clock::now_ms());

                if remaining == 0 { return Err(AppError::invalid("decision", "already expired")); }

                if cache.len() >= cache.capacity() && cache.get(&write.key).is_none() { return Err(AppError::invalid("decision", "decision store is full")); }

                db.execute("INSERT OR REPLACE INTO verdicts (key, expires_ms, payload) VALUES (?1, ?2, ?3)", params![hex, verdict.expires_ms as i64, payload])?;
                cache.put_for(write.key, verdict.clone(), remaining);

            }
            None => {

                db.execute("DELETE FROM verdicts WHERE key = ?1", params![hex])?;
                cache.remove(&write.key);

            }
        }

        Ok(())

    }

    pub(super) fn load ( db: &Db, cache: &Cache<Key, Arc<Verdict>> ) -> AppResult<usize> {

        let now = Clock::now_ms();
        let rows = db.rows("SELECT key, payload FROM verdicts WHERE expires_ms > ?1 ORDER BY expires_ms DESC", params![now as i64], |row| Ok(( row.get::<_, String>(0)?, row.get::<_, String>(1)? )))?;
        let mut loaded = 0;

        cache.clear();

        for ( hex, payload ) in rows {

            let ( Some(key), Ok(verdict) ) = ( Self::parse_key(&hex), Json::parse::<Verdict>(payload.as_bytes()) ) else { continue; };
            let remaining = verdict.expires_ms.saturating_sub(now);

            if remaining > 0 && cache.put_for(key, Arc::new(verdict), remaining) { loaded += 1; }

        }

        Ok(loaded)

    }

    pub fn list ( &self, limit: usize ) -> AppResult<Vec<Verdict>> {

        let now = Clock::now_ms();
        let rows = self.db.rows("SELECT payload FROM verdicts WHERE expires_ms > ?1 ORDER BY expires_ms DESC LIMIT ?2", params![now as i64, limit as i64], |row| row.get::<_, String>(0))?;

        Ok(rows.iter().filter_map(|payload| Json::parse::<Verdict>(payload.as_bytes()).ok()).collect())

    }

    pub fn sweep ( &self ) -> AppResult<usize> {

        let removed = self.db.execute("DELETE FROM verdicts WHERE expires_ms <= ?1", params![Clock::now_ms() as i64])?;

        self.cache.sweep();

        Ok(removed)

    }

    pub fn parse_key ( hex: &str ) -> Option<Key> {

        if hex.len() != 64 || !hex.bytes().all(|byte| byte.is_ascii_hexdigit()) { return None; }

        let mut key = [0u8; 32];

        for ( index, slot ) in key.iter_mut().enumerate() { *slot = u8::from_str_radix(&hex[index * 2..index * 2 + 2], 16).ok()?; }

        Some(key)

    }

    pub(super) fn counter () -> Arc<AtomicU64> {

        Arc::new(AtomicU64::new(0))

    }

    pub(super) fn bump ( counter: &AtomicU64 ) {

        counter.fetch_add(1, Ordering::Relaxed);

    }

}
