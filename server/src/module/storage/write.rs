use std::{path::Path, os::unix::fs::OpenOptionsExt, sync::{Arc, atomic::Ordering}, thread, time::Duration};
use rusqlite::{Connection, params};
use tokio::sync::mpsc;
use crate::core::error::{AppError, AppFail, AppResult};
use super::arch::{Counters, Event, Message, Reservation, Store, StoreGuard};

impl Store {
    pub fn disabled () -> Self { Self { sender: None, counters: Arc::default() } }

    pub fn open ( path: &Path, capacity: usize, retention: usize, synchronous: &str, gate: crate::core::WriteGate ) -> AppResult<(Self, StoreGuard)> {
        std::fs::OpenOptions::new().create(true).append(true).mode(0o600).open(path)
            .or_fail("Cannot create private lifecycle database")?;
        let mut connection = Connection::open(path).or_fail("Cannot open lifecycle database")?;
        connection.busy_timeout(Duration::from_millis(100)).or_fail("Cannot set database timeout")?;
        connection.execute_batch(
            "PRAGMA journal_mode=WAL; PRAGMA max_page_count=262144;
             CREATE TABLE IF NOT EXISTS events (
                 id INTEGER PRIMARY KEY, request_id TEXT NOT NULL, sequence INTEGER NOT NULL,
                 timestamp_ms INTEGER NOT NULL, stage TEXT NOT NULL, payload TEXT NOT NULL
             );
             CREATE INDEX IF NOT EXISTS events_request ON events(request_id, sequence);"
        ).or_fail("Cannot initialize lifecycle database")?;
        connection.pragma_update(None,"synchronous",if synchronous=="full" {"FULL"} else {"NORMAL"})
            .or_fail("Cannot configure lifecycle durability")?;
        super::retention::prepare(&connection)?;
        let (sender, mut receiver) = mpsc::channel(capacity);
        let counters=Arc::new(Counters::default());
        let stats=counters.clone();
        let thread=thread::Builder::new().name("aegisx-store".into()).spawn(move || {
            let mut written=0;
            let mut stopping=false;
            while !stopping {
                let Some(message)=receiver.blocking_recv() else { break; };
                let mut batches=Vec::with_capacity(32);
                match message { Message::Stop=>break, Message::Batch(events)=>batches.push(events) }
                while batches.len()<32 {
                    match receiver.try_recv() {
                        Ok(Message::Batch(events))=>batches.push(events),
                        Ok(Message::Stop)=>{ stopping=true; break; },
                        Err(_)=>break,
                    }
                }
                let count=batches.iter().map(Vec::len).sum::<usize>();
                // Serialize once, outside the SQL transaction; retry the same bounded batch.
                let encoded=batches.iter().flatten().map(serde_json::to_string)
                    .collect::<Result<Vec<_>,_>>().or_fail("Cannot encode lifecycle batch")?;
                let mut retry: u32=0;
                loop {
                    match persist(&mut connection,&batches,&encoded,&gate) {
                        Ok(())=>{
                            stats.unhealthy.store(false,Ordering::Release);
                            stats.committed.fetch_add(count as u64,Ordering::Relaxed);
                            stats.batches.fetch_add(batches.len() as u64,Ordering::Relaxed);
                            break;
                        }
                        Err(error)=>{
                            stats.unhealthy.store(true,Ordering::Release);
                            stats.retries.fetch_add(1,Ordering::Relaxed);
                            retry+=1;
                            if retry.is_power_of_two() { tracing::warn!(error=%error,retry,"Lifecycle storage unavailable; retaining batch and closing required admissions"); }
                            if stats.stopping.load(Ordering::Acquire) {
                                stats.dropped.fetch_add(batches.len() as u64,Ordering::Relaxed);
                                while let Ok(message)=receiver.try_recv() {
                                    if let Message::Batch(_)=message { stats.dropped.fetch_add(1,Ordering::Relaxed); }
                                }
                                return Err(error);
                            }
                            thread::sleep(Duration::from_millis((retry as u64*10).min(250)));
                        }
                    }
                }
                written+=count;
                if written>=1024 {
                    let _guard=gate.lock();
                    if let Err(error)=super::retention::prune(&mut connection,retention) {
                        tracing::warn!(%error,"Lifecycle retention deferred");
                    }
                    written=0;
                }
            }
            connection.execute_batch("PRAGMA wal_checkpoint(TRUNCATE)").or_fail("Cannot checkpoint lifecycle database")?;
            Ok(())
        }).or_fail("Cannot start lifecycle writer")?;
        Ok((Self {sender:Some(sender.clone()),counters:counters.clone()},StoreGuard {sender,counters,thread:Some(thread)}))
    }
    pub fn enabled ( &self ) -> bool { self.sender.is_some() }
    fn ready ( &self ) -> bool {
        !self.counters.unhealthy.load(Ordering::Acquire) && !self.counters.stopping.load(Ordering::Acquire)
    }
    pub fn try_reserve ( &self ) -> Option<Reservation> {
        if !self.ready() { return None; }
        self.sender.as_ref()?.clone().try_reserve_owned().ok().map(|permit| Reservation {permit})
    }
    pub async fn wait_reserve ( &self ) -> Option<Reservation> {
        if !self.ready() { return None; }
        let permit=self.sender.as_ref()?.clone().reserve_owned().await.ok()?;
        self.ready().then_some(Reservation {permit})
    }
    pub fn reservation_failed ( &self, required: bool ) {
        if required {self.counters.rejected.fetch_add(1,Ordering::Relaxed);}
        else {self.counters.dropped.fetch_add(1,Ordering::Relaxed);}
    }
    pub fn dropped ( &self ) -> u64 { self.counters.dropped.load(Ordering::Relaxed) }
    pub fn stats ( &self ) -> serde_json::Value {
        let read=|counter:&std::sync::atomic::AtomicU64|counter.load(Ordering::Relaxed);
        serde_json::json!({"enabled":self.enabled(),"healthy":!self.counters.unhealthy.load(Ordering::Acquire) && self.sender.as_ref().is_none_or(|sender|!sender.is_closed()),
            "committed_events":read(&self.counters.committed),"committed_batches":read(&self.counters.batches),
            "pressure_rejections":read(&self.counters.rejected),"dropped_batches":read(&self.counters.dropped),"write_retries":read(&self.counters.retries),
            "used_slots":self.sender.as_ref().map_or(0,|s|s.max_capacity()-s.capacity()),
            "capacity":self.sender.as_ref().map_or(0,|s|s.max_capacity())})
    }
}
fn persist ( connection: &mut Connection, batches: &[Vec<Event>], encoded: &[String], gate: &crate::core::WriteGate ) -> AppResult<()> {
    let _guard=gate.lock();
    let transaction=connection.transaction().or_fail("Cannot begin lifecycle batch")?;
    {
        let mut statement=transaction.prepare_cached(
            "INSERT INTO events(request_id,sequence,timestamp_ms,stage,payload) VALUES (?1,?2,?3,?4,?5)"
        ).or_fail("Cannot prepare lifecycle insert")?;
        let mut index=transaction.prepare_cached("INSERT INTO event_journeys(request_id,last_id) VALUES (?1,?2)
            ON CONFLICT(request_id) DO UPDATE SET last_id=excluded.last_id").or_fail("Cannot prepare journey index")?;
        let mut encoded=encoded.iter();
        for batch in batches {
            for event in batch {
                statement.execute(params![event.request_id,event.sequence,event.timestamp_ms as i64,event.stage,encoded.next()])
                    .or_fail("Cannot persist lifecycle event")?;
            }
            if let Some(event)=batch.last() {
                index.execute(params![event.request_id,transaction.last_insert_rowid()]).or_fail("Cannot update journey index")?;
            }
        }
    }
    transaction.commit().or_fail("Cannot commit lifecycle batch")
}
impl StoreGuard {
    pub fn finish ( mut self ) -> AppResult<()> {
        self.counters.stopping.store(true,Ordering::Release);
        let _=self.sender.blocking_send(Message::Stop);
        if let Some(thread)=self.thread.take() { thread.join().map_err(|_|AppError::invalid("Lifecycle writer panicked"))??; }
        Ok(())
    }
}
