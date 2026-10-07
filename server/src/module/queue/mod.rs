use std::{future::Future, sync::{Arc, atomic::{AtomicU64, Ordering}}, time::Duration};
use serde_json::{Value, json};
use tokio::{sync::{OwnedSemaphorePermit, Semaphore}, time::Instant};

use crate::module::config::QueueConfig;

/// A shared bound on live waiting requests, distinct from active forwarding and jobs.
pub struct Queue {
    slots: Arc<Semaphore>,
    capacity: usize,
    timeout: Duration,
    counters: Arc<Counters>,
}
#[derive(Default)]
struct Counters {
    entered: AtomicU64, completed: AtomicU64, resumed: AtomicU64, full: AtomicU64,
    timed_out: AtomicU64, unavailable: AtomicU64, cancelled: AtomicU64, total_us: AtomicU64,
}
struct Waiting {
    _slot: OwnedSemaphorePermit, counters: Arc<Counters>, started: Instant, finished: bool,
}
impl Drop for Waiting {
    fn drop ( &mut self ) {
        self.counters.completed.fetch_add(1, Ordering::Relaxed);
        self.counters.total_us.fetch_add(self.started.elapsed().as_micros() as u64, Ordering::Relaxed);
        if !self.finished { self.counters.cancelled.fetch_add(1, Ordering::Relaxed); }
    }
}
impl Queue {
    pub fn new ( config: &QueueConfig ) -> Self {
        Self { slots: Arc::new(Semaphore::new(config.capacity)), capacity: config.capacity,
            timeout: Duration::from_millis(config.timeout_ms), counters: Arc::default() }
    }
    pub fn enabled ( &self ) -> bool { self.capacity > 0 }

    pub async fn wait<T> ( &self, deadline: &mut Option<Instant>, future: impl Future<Output=Option<T>> ) -> Option<T> {
        let end = *deadline.get_or_insert_with(|| Instant::now() + self.timeout);
        if Instant::now() >= end {
            self.counters.timed_out.fetch_add(1, Ordering::Relaxed);
            return None;
        }
        let Ok(slot) = self.slots.clone().try_acquire_owned() else {
            self.counters.full.fetch_add(1, Ordering::Relaxed);
            return None;
        };
        self.counters.entered.fetch_add(1, Ordering::Relaxed);
        let mut waiting = Waiting { _slot: slot, counters: self.counters.clone(), started: Instant::now(), finished: false };
        let result = match tokio::time::timeout_at(end, future).await {
            Ok(Some(value)) => { self.counters.resumed.fetch_add(1, Ordering::Relaxed); Some(value) }
            Ok(None) => { self.counters.unavailable.fetch_add(1, Ordering::Relaxed); None }
            Err(_) => { self.counters.timed_out.fetch_add(1, Ordering::Relaxed); None }
        };
        waiting.finished = true;
        result
    }
    pub fn stats ( &self ) -> Value {
        let read = |value: &AtomicU64| value.load(Ordering::Relaxed);
        json!({"capacity":self.capacity, "waiting":self.capacity-self.slots.available_permits(),
            "timeout_ms":self.timeout.as_millis() as u64, "entered":read(&self.counters.entered),
            "completed":read(&self.counters.completed), "resumed":read(&self.counters.resumed), "full":read(&self.counters.full),
            "timed_out":read(&self.counters.timed_out), "unavailable":read(&self.counters.unavailable),
            "cancelled":read(&self.counters.cancelled), "total_us":read(&self.counters.total_us)})
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn shared_capacity_deadline_and_cancellation_release_slots () {
        tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap().block_on(async {
            let queue=Arc::new(Queue::new(&QueueConfig {capacity:1,timeout_ms:40}));
            let first=queue.clone();
            let task=tokio::spawn(async move {first.wait::<()>(&mut None,std::future::pending()).await});
            tokio::task::yield_now().await;
            assert_eq!(queue.stats()["waiting"],1);
            assert!(queue.wait(&mut None,async {Some(1)}).await.is_none());
            task.abort(); let _=task.await;
            assert_eq!(queue.stats()["waiting"],0);
            assert_eq!(queue.stats()["cancelled"],1);
            let mut deadline=None;
            assert_eq!(queue.wait(&mut deadline,async {Some(1)}).await,Some(1));
            tokio::time::sleep(Duration::from_millis(45)).await;
            assert!(queue.wait(&mut deadline,async {Some(2)}).await.is_none());
            assert_eq!(queue.stats()["timed_out"],1);
        });
    }
}
