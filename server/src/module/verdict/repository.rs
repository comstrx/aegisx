use std::{path::Path, sync::{Arc, atomic::{AtomicU64, Ordering}}, thread::{self, JoinHandle}, time::Duration};
use moka::sync::Cache;
use rusqlite::{Connection, params};
use tokio::sync::{mpsc, oneshot};
use crate::core::{error::{AppError, AppFail, AppResult}, time::now_ms};
use crate::module::{cache::Key, config::CacheConfig};

use super::{Cancellation, Verdict};
enum Command {
    Cancel(Cancellation, Option<u64>, oneshot::Sender<AppResult<Cancellation>>),
    Acknowledge(String,String,oneshot::Sender<AppResult<Cancellation>>),
    Cancellations(oneshot::Sender<AppResult<Vec<Cancellation>>>),
    Read(Key, oneshot::Sender<AppResult<Option<Arc<Verdict>>>>),
    Put(Key, Verdict, Option<u64>, oneshot::Sender<AppResult<bool>>),
    Revoke(Key, oneshot::Sender<AppResult<()>>),
    List(oneshot::Sender<AppResult<Vec<Verdict>>>),
    Purge(oneshot::Sender<AppResult<()>>),
    Stop,
}
struct Shared {
    cache: Cache<Key, Option<Arc<Verdict>>>,
    hits: AtomicU64,
    misses: AtomicU64,
    failures: AtomicU64,
    read_failures: AtomicU64,
    database_reads: AtomicU64,
    coalesced: AtomicU64,
    generation: AtomicU64,
}
#[derive(Clone)]
pub struct Verdicts { sender: mpsc::Sender<Command>, shared: Arc<Shared>, timeout: Duration, lookup_timeout: Duration }
pub struct VerdictGuard { sender: mpsc::Sender<Command>, thread: JoinHandle<()> }

impl Verdicts {
    pub fn open ( path: &Path, config: &CacheConfig, gate: crate::core::WriteGate ) -> AppResult<(Self, VerdictGuard, Vec<u8>)> {
        let connection = Connection::open(path).or_fail("Cannot open verdict repository")?;
        connection.busy_timeout(Duration::from_millis(config.write_timeout_ms)).or_fail("Cannot set verdict timeout")?;
        connection.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=FULL;
            CREATE TABLE IF NOT EXISTS verdicts(key TEXT PRIMARY KEY, expires_ms INTEGER NOT NULL, payload TEXT NOT NULL);
            CREATE INDEX IF NOT EXISTS verdicts_expiry ON verdicts(expires_ms);
            CREATE TABLE IF NOT EXISTS cancellations(action_id TEXT PRIMARY KEY,request_id TEXT NOT NULL,route TEXT NOT NULL,created_ms INTEGER NOT NULL,expires_ms INTEGER NOT NULL,payload TEXT NOT NULL);
            CREATE INDEX IF NOT EXISTS cancellation_request ON cancellations(request_id,route);
            CREATE TABLE IF NOT EXISTS private_settings(key TEXT PRIMARY KEY, value BLOB NOT NULL);")
            .or_fail("Cannot initialize verdict repository")?;
        let mut key = [0u8;32];
        openssl::rand::rand_bytes(&mut key).or_fail("Cannot generate identity key")?;
        connection.execute("INSERT OR IGNORE INTO private_settings VALUES ('identity_key', ?1)", [&key[..]])
            .or_fail("Cannot persist identity key")?;
        let key = connection.query_row("SELECT value FROM private_settings WHERE key='identity_key'", [], |row| row.get::<_,Vec<u8>>(0))
            .or_fail("Cannot read identity key")?;
        if key.len() != 32 { return Err(AppError::invalid("Invalid stored identity key")); }
        connection.set_prepared_statement_cache_capacity(8);
        let shared = Arc::new(Shared {
            cache: Cache::builder().max_capacity(config.max_entries).time_to_live(Duration::from_millis(config.decision_ttl_ms)).build(),
            hits: AtomicU64::new(0), misses: AtomicU64::new(0), failures: AtomicU64::new(0), read_failures: AtomicU64::new(0), database_reads: AtomicU64::new(0), coalesced: AtomicU64::new(0), generation: AtomicU64::new(0),
        });
        let (sender, mut receiver) = mpsc::channel(1024);
        let state = shared.clone();
        let thread = thread::Builder::new().name("aegisx-verdicts".into()).spawn(move || {
            let mut operations = 0;
            while let Some(command) = receiver.blocking_recv() {
                let write=if matches!(command,Command::Cancel(..)|Command::Acknowledge(..)|Command::Put(..)|Command::Revoke(..)) {
                    Some(gate.lock())
                } else {None};
                match command {
                    Command::Stop => break,
                    Command::Cancel(value, generation, reply) => {
                        let result=if generation.is_some_and(|version|version!=state.generation.load(Ordering::Acquire)) {
                            Err(AppError::invalid("Cancellation superseded by operator"))
                        } else {super::cancellation::create(&connection,value)};
                        let _=reply.send(result);
                    }
                    Command::Acknowledge(id,state,reply) => { let _=reply.send(super::cancellation::acknowledge(&connection,&id,&state)); }
                    Command::Cancellations(reply) => { let _=reply.send(super::cancellation::list(&connection)); }
                    Command::Read(key, reply) => {
                        if reply.is_closed() { continue; }
                        let result = (|| {
                            // Recheck on the serial owner: queued misses share the previous read or committed write.
                            // No delayed reader can overwrite a newer Put/Revoke publication.
                            if let Some(value) = state.cache.get(&key) {
                                state.coalesced.fetch_add(1, Ordering::Relaxed);
                                return Ok(value.filter(|value| value.expires_ms > now_ms()));
                            }
                            state.database_reads.fetch_add(1, Ordering::Relaxed);
                            use rusqlite::OptionalExtension;
                            let mut statement = connection.prepare_cached(
                                "SELECT expires_ms, CASE WHEN length(CAST(payload AS BLOB))<=4096 THEN payload END FROM verdicts WHERE key=?1")
                                .or_fail("Cannot prepare verdict lookup")?;
                            let key_text = crate::core::domain::hex(&key);
                            let stored = statement.query_row([&key_text],
                                |row| Ok((row.get::<_,i64>(0)?, row.get::<_,Option<String>>(1)?)))
                                .optional().or_fail("Verdict lookup failed")?;
                            let value = stored.map(|(expiry, payload)| -> AppResult<Arc<Verdict>> {
                                let payload = payload.ok_or_else(|| AppError::invalid("Oversized verdict record"))?;
                                let value: Verdict = serde_json::from_str(&payload).or_fail("Invalid stored verdict")?;
                                value.validate(&key_text, expiry)?;
                                Ok(Arc::new(value))
                            }).transpose()?.filter(|value| value.expires_ms > now_ms());
                            state.cache.insert(key, value.clone());
                            Ok(value)
                        })();
                        if result.is_err() { state.read_failures.fetch_add(1, Ordering::Relaxed); }
                        let _ = reply.send(result);
                    }
                    Command::Put(key, value, generation, reply) => {
                        let result = (|| {
                            if generation.is_some_and(|version| version != state.generation.load(Ordering::Acquire)) { return Ok(false); }
                            value.validate_key(&key)?;
                            if generation.is_none() { state.generation.fetch_add(1, Ordering::AcqRel); }
                            let payload = serde_json::to_string(&value).or_fail("Cannot encode verdict")?;
                            // A single writer publishes the cache only after a successful commit.
                            connection.prepare_cached("INSERT INTO verdicts VALUES (?1,?2,?3) ON CONFLICT(key) DO UPDATE SET expires_ms=excluded.expires_ms,payload=excluded.payload").or_fail("Cannot prepare verdict write")?.execute(
                                params![value.key, value.expires_ms as i64, payload]).or_fail("Cannot persist verdict")?;
                            state.cache.insert(key, Some(Arc::new(value)));
                            Ok(true)
                        })();
                        if result.is_err() { state.failures.fetch_add(1, Ordering::Relaxed); }
                        let _ = reply.send(result);
                    }
                    Command::Revoke(key, reply) => {
                        state.generation.fetch_add(1, Ordering::AcqRel);
                        let result = connection.execute("DELETE FROM verdicts WHERE key=?1", [crate::core::domain::hex(&key)])
                            .or_fail("Cannot revoke verdict").map(|_| state.cache.insert(key, None));
                        if result.is_err() { state.failures.fetch_add(1, Ordering::Relaxed); }
                        let _ = reply.send(result);
                    }
                    Command::List(reply) => {
                        let result = (|| {
                            let mut statement = connection.prepare_cached("SELECT key,expires_ms,CASE WHEN length(CAST(payload AS BLOB))<=4096 THEN payload END FROM verdicts WHERE expires_ms>?1 ORDER BY expires_ms DESC LIMIT 200").or_fail("Cannot list verdicts")?;
                            let rows = statement.query_map([now_ms() as i64],
                                |row| Ok((row.get::<_,String>(0)?,row.get::<_,i64>(1)?,row.get::<_,Option<String>>(2)?))).or_fail("Cannot query verdicts")?;
                            rows.map(|row| {
                                let (key,expiry,payload) = row.or_fail("Cannot read verdict")?;
                                let value: Verdict = serde_json::from_str(&payload.ok_or_else(|| AppError::invalid("Oversized verdict record"))?).or_fail("Invalid verdict")?;
                                value.validate(&key,expiry)?;
                                Ok(value)
                            }).collect()
                        })();
                        let _ = reply.send(result);
                    }
                    Command::Purge(reply) => { state.cache.invalidate_all(); let _ = reply.send(Ok(())); }
                }
                drop(write);
                operations += 1;
                if operations % 256 == 0 { let _guard=gate.lock(); let _ = connection.execute("DELETE FROM verdicts WHERE expires_ms<=?1", [now_ms() as i64]); }
            }
        }).or_fail("Cannot start verdict repository")?;
        Ok((Self { sender: sender.clone(), shared, timeout: Duration::from_millis(config.write_timeout_ms), lookup_timeout: Duration::from_millis(config.lookup_timeout_ms) },
            VerdictGuard { sender, thread }, key))
    }

    async fn receive<T> ( &self, reply: oneshot::Receiver<AppResult<T>> ) -> AppResult<T> {
        tokio::time::timeout(self.timeout, reply).await.map_err(|_| AppError::invalid("Verdict repository timed out"))?
            .map_err(|_| AppError::invalid("Verdict repository unavailable"))?
    }
    pub fn generation ( &self ) -> u64 { self.shared.generation.load(Ordering::Acquire) }
    pub async fn get ( &self, key: Key ) -> AppResult<Option<Arc<Verdict>>> {
        if let Some(value) = self.shared.cache.get(&key) {
            self.shared.hits.fetch_add(1, Ordering::Relaxed);
            return Ok(value.filter(|value| value.expires_ms > now_ms()));
        }
        self.shared.misses.fetch_add(1, Ordering::Relaxed);
        let (sender, reply) = oneshot::channel();
        self.sender.try_send(Command::Read(key, sender)).map_err(|_| AppError::invalid("Verdict queue full"))?;
        tokio::time::timeout(self.lookup_timeout, reply).await.map_err(|_| AppError::invalid("Verdict lookup timed out"))?
            .map_err(|_| AppError::invalid("Verdict repository unavailable"))?
    }
    pub fn submit ( &self, key: Key, value: Verdict, generation: Option<u64> ) -> AppResult<oneshot::Receiver<AppResult<bool>>> {
        value.validate_key(&key)?;
        let (sender, reply) = oneshot::channel();
        self.sender.try_send(Command::Put(key, value, generation, sender)).map_err(|_| AppError::invalid("Verdict queue full"))?;
        Ok(reply)
    }
    pub async fn put ( &self, key: Key, value: Verdict ) -> AppResult<bool> { self.receive(self.submit(key, value, None)?).await }
    pub async fn revoke ( &self, key: Key ) -> AppResult<()> {
        let (sender, reply) = oneshot::channel();
        self.sender.try_send(Command::Revoke(key, sender)).map_err(|_| AppError::invalid("Verdict queue full"))?;
        self.receive(reply).await
    }
    pub async fn list ( &self ) -> AppResult<Vec<Verdict>> {
        let (sender, reply) = oneshot::channel();
        self.sender.try_send(Command::List(sender)).map_err(|_| AppError::invalid("Verdict queue full"))?;
        self.receive(reply).await
    }
    pub async fn purge ( &self ) -> AppResult<()> {
        let (sender, reply) = oneshot::channel();
        self.sender.try_send(Command::Purge(sender)).map_err(|_| AppError::invalid("Verdict queue full"))?;
        self.receive(reply).await
    }
    pub fn submit_cancellation ( &self, value: Cancellation, generation: Option<u64> ) -> AppResult<oneshot::Receiver<AppResult<Cancellation>>> {
        let (sender,reply)=oneshot::channel();
        self.sender.try_send(Command::Cancel(value,generation,sender)).map_err(|_|AppError::invalid("Cancellation queue full"))?;
        Ok(reply)
    }
    pub async fn cancel ( &self, value: Cancellation ) -> AppResult<Cancellation> { self.receive(self.submit_cancellation(value,None)?).await }
    pub async fn acknowledge ( &self, id: String, state: String ) -> AppResult<Cancellation> {
        let (sender,reply)=oneshot::channel();
        self.sender.try_send(Command::Acknowledge(id,state,sender)).map_err(|_|AppError::invalid("Cancellation queue full"))?;
        self.receive(reply).await
    }
    pub async fn cancellations ( &self ) -> AppResult<Vec<Cancellation>> {
        let (sender,reply)=oneshot::channel();
        self.sender.try_send(Command::Cancellations(sender)).map_err(|_|AppError::invalid("Cancellation queue full"))?;
        self.receive(reply).await
    }
    pub fn stats ( &self ) -> serde_json::Value {
        serde_json::json!({"hits":self.shared.hits.load(Ordering::Relaxed),"misses":self.shared.misses.load(Ordering::Relaxed),
            "write_failures":self.shared.failures.load(Ordering::Relaxed),
            "read_failures":self.shared.read_failures.load(Ordering::Relaxed),"database_reads":self.shared.database_reads.load(Ordering::Relaxed),
            "coalesced_reads":self.shared.coalesced.load(Ordering::Relaxed),"cached_keys":self.shared.cache.entry_count(),
            "queued":self.sender.max_capacity()-self.sender.capacity(),"generation":self.generation()})
    }
}
impl VerdictGuard {
    pub fn finish ( self ) -> AppResult<()> {
        let _ = self.sender.blocking_send(Command::Stop);
        self.thread.join().map_err(|_| AppError::invalid("Verdict worker panicked"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cancellation_checks_operator_generation_inside_the_serial_writer () {
        let path=std::env::temp_dir().join(format!("aegisx-generation-{}.db",uuid::Uuid::new_v4()));
        let (repository,guard,_)=Verdicts::open(&path,&CacheConfig::default(),Default::default()).unwrap();
        let generation=repository.generation();
        let (reply,received)=oneshot::channel();
        repository.sender.try_send(Command::Revoke([0;32],reply)).unwrap();
        received.blocking_recv().unwrap().unwrap();
        let action=Cancellation::new(uuid::Uuid::new_v4().to_string(),"jobs".into(),"background_risk".into(),10000);
        let result=repository.submit_cancellation(action,Some(generation)).unwrap().blocking_recv().unwrap();
        assert!(result.is_err());
        let (reply,received)=oneshot::channel();
        repository.sender.try_send(Command::Cancellations(reply)).unwrap();
        assert!(received.blocking_recv().unwrap().unwrap().is_empty());
        guard.finish().unwrap();
        drop(repository);
        for suffix in ["","-wal","-shm"] {let _=std::fs::remove_file(format!("{}{suffix}",path.display()));}
    }
    fn stored ( key: Key ) -> Verdict {
        let now=now_ms();
        Verdict {key:crate::core::domain::hex(&key),actor:"ab".repeat(32),route:"api".into(),
            reason:"test".into(),source:"operator".into(),created_ms:now,expires_ms:now+60000,request_id:None}
    }
    fn read ( repository: &Verdicts, key: Key ) -> AppResult<Option<Arc<Verdict>>> {
        let (sender,reply)=oneshot::channel();
        repository.sender.try_send(Command::Read(key,sender)).unwrap();
        reply.blocking_recv().unwrap()
    }
    fn purge ( repository: &Verdicts ) {
        let (sender,reply)=oneshot::channel();
        repository.sender.try_send(Command::Purge(sender)).unwrap();
        reply.blocking_recv().unwrap().unwrap();
    }
    fn cleanup ( path: &std::path::Path ) {
        for suffix in ["","-wal","-shm"] {let _=std::fs::remove_file(format!("{}{suffix}",path.display()));}
    }
    #[test]
    fn queued_misses_coalesce_and_commits_supersede_negative_entries () {
        let path=std::env::temp_dir().join(format!("aegisx-coalesced-{}.db",uuid::Uuid::new_v4()));
        let (repository,guard,_)=Verdicts::open(&path,&CacheConfig::default(),Default::default()).unwrap();
        let key=[8;32];
        let replies=(0..100).map(|_| {
            let (sender,reply)=oneshot::channel();
            repository.sender.try_send(Command::Read(key,sender)).unwrap();
            reply
        }).collect::<Vec<_>>();
        for reply in replies {assert!(reply.blocking_recv().unwrap().unwrap().is_none());}
        assert_eq!(repository.stats()["database_reads"],1);
        assert_eq!(repository.stats()["coalesced_reads"],99);
        repository.submit(key,stored(key),None).unwrap().blocking_recv().unwrap().unwrap();
        let first=read(&repository,key).unwrap().unwrap();
        let second=read(&repository,key).unwrap().unwrap();
        assert!(Arc::ptr_eq(&first,&second));
        purge(&repository);
        assert!(read(&repository,key).unwrap().is_some());
        assert_eq!(repository.stats()["database_reads"],2);
        let (sender,reply)=oneshot::channel();
        repository.sender.try_send(Command::Revoke(key,sender)).unwrap();
        reply.blocking_recv().unwrap().unwrap();
        assert!(read(&repository,key).unwrap().is_none());
        guard.finish().unwrap();drop(repository);cleanup(&path);
    }
    #[test]
    fn invalid_persistent_records_never_become_cached_allows_or_denials () {
        let path=std::env::temp_dir().join(format!("aegisx-integrity-{}.db",uuid::Uuid::new_v4()));
        let (repository,guard,_)=Verdicts::open(&path,&CacheConfig::default(),Default::default()).unwrap();
        let key=[9;32];let value=stored(key);
        assert!(repository.submit([7;32],value.clone(),None).is_err());
        let connection=Connection::open(&path).unwrap();
        for (payload,expiry) in [
            (serde_json::to_string(&stored([7;32])).unwrap(),value.expires_ms as i64),
            (serde_json::to_string(&value).unwrap(),value.expires_ms as i64+1),
            ("x".repeat(5000),value.expires_ms as i64),
        ] {
            connection.execute("INSERT OR REPLACE INTO verdicts VALUES(?1,?2,?3)",params![value.key,expiry,payload]).unwrap();
            purge(&repository);
            assert!(read(&repository,key).is_err());
            assert!(read(&repository,key).is_err());
        }
        assert_eq!(repository.stats()["read_failures"],6);
        repository.submit(key,value,None).unwrap().blocking_recv().unwrap().unwrap();
        assert!(read(&repository,key).unwrap().is_some());
        // Failed database commit must preserve the last acknowledged cache value.
        connection.execute_batch("CREATE TRIGGER reject_update BEFORE UPDATE ON verdicts BEGIN SELECT RAISE(ABORT,'fixture'); END").unwrap();
        let mut replacement=stored(key);replacement.reason="uncommitted".into();
        assert!(repository.submit(key,replacement,None).unwrap().blocking_recv().unwrap().is_err());
        assert_eq!(read(&repository,key).unwrap().unwrap().reason,"test");
        drop(connection);guard.finish().unwrap();drop(repository);cleanup(&path);
    }
    #[test]
    fn damaged_identity_secret_fails_at_startup () {
        let path=std::env::temp_dir().join(format!("aegisx-secret-{}.db",uuid::Uuid::new_v4()));
        let connection=Connection::open(&path).unwrap();
        connection.execute_batch("CREATE TABLE private_settings(key TEXT PRIMARY KEY,value BLOB NOT NULL); INSERT INTO private_settings VALUES('identity_key',x'01')").unwrap();
        drop(connection);
        assert!(Verdicts::open(&path,&CacheConfig::default(),Default::default()).is_err());
        cleanup(&path);
    }

}
