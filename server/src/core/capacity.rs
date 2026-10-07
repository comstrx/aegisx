//! Exact admission bounds with an atomic idle path and notifications only under pressure.
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use tokio::sync::Notify;

pub struct Capacity {
    limit: usize,
    active: AtomicUsize,
    waiting: AtomicUsize,
    changed: Notify,
}

pub struct Permit { capacity: Arc<Capacity> }

impl Capacity {
    pub fn new ( limit: usize ) -> Self {
        assert!(limit > 0);
        Self { limit, active: AtomicUsize::new(0), waiting: AtomicUsize::new(0), changed: Notify::new() }
    }

    fn reserve ( self: &Arc<Self> ) -> Option<Permit> {
        self.active.fetch_update(Ordering::SeqCst, Ordering::SeqCst,
            |active| (active < self.limit).then_some(active + 1)).ok()?;
        Some(Permit { capacity: self.clone() })
    }

    pub fn try_acquire ( self: &Arc<Self> ) -> Option<Permit> {
        if self.waiting.load(Ordering::SeqCst) != 0 { return None; }
        self.reserve()
    }

    pub async fn acquire ( self: &Arc<Self> ) -> Permit {
        struct Waiting<'a>(&'a Capacity);
        impl Drop for Waiting<'_> {
            fn drop ( &mut self ) {
                self.0.waiting.fetch_sub(1, Ordering::SeqCst);
                // Cancellation may consume a notification while a slot is available.
                if self.0.active.load(Ordering::SeqCst) < self.0.limit { self.0.changed.notify_one(); }
            }
        }
        self.waiting.fetch_add(1, Ordering::SeqCst);
        let _waiting = Waiting(self);
        loop {
            let notified = self.changed.notified();
            tokio::pin!(notified);
            // Register before checking capacity: a concurrent release cannot be lost.
            notified.as_mut().enable();
            if let Some(permit) = self.reserve() { return permit; }
            notified.await;
        }
    }
}

impl Drop for Permit {
    fn drop ( &mut self ) {
        self.capacity.active.fetch_sub(1, Ordering::SeqCst);
        if self.capacity.waiting.load(Ordering::SeqCst) != 0 { self.capacity.changed.notify_one(); }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cancellation_hands_available_capacity_to_another_waiter () {
        tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap().block_on(async {
            let capacity = Arc::new(Capacity::new(1));
            let held = capacity.try_acquire().unwrap();
            let first = capacity.clone();
            let cancelled = tokio::spawn(async move { first.acquire().await });
            tokio::task::yield_now().await;
            let second = capacity.clone();
            let remaining = tokio::spawn(async move { second.acquire().await });
            tokio::task::yield_now().await;
            drop(held);
            cancelled.abort();
            let _ = cancelled.await;
            let permit = tokio::time::timeout(std::time::Duration::from_secs(1), remaining).await.unwrap().unwrap();
            assert_eq!(capacity.active.load(Ordering::SeqCst), 1);
            drop(permit);
            assert_eq!(capacity.active.load(Ordering::SeqCst), 0);
            assert_eq!(capacity.waiting.load(Ordering::SeqCst), 0);
        });
    }

    #[test]
    fn concurrent_admission_never_exceeds_the_exact_global_limit () {
        tokio::runtime::Builder::new_multi_thread().worker_threads(4).enable_all().build().unwrap().block_on(async {
            let capacity = Arc::new(Capacity::new(7));
            let observed = Arc::new(AtomicUsize::new(0));
            let mut tasks = Vec::new();
            for _ in 0..64 {
                let capacity = capacity.clone();
                let observed = observed.clone();
                tasks.push(tokio::spawn(async move {
                    for _ in 0..64 {
                        let permit = match capacity.try_acquire() { Some(permit) => permit, None => capacity.acquire().await };
                        let active = observed.fetch_add(1, Ordering::SeqCst) + 1;
                        assert!(active <= 7);
                        tokio::task::yield_now().await;
                        observed.fetch_sub(1, Ordering::SeqCst);
                        drop(permit);
                    }
                }));
            }
            for task in tasks { tokio::time::timeout(std::time::Duration::from_secs(10), task).await.unwrap().unwrap(); }
            assert_eq!(capacity.active.load(Ordering::SeqCst), 0);
            assert_eq!(capacity.waiting.load(Ordering::SeqCst), 0);
        });
    }
}
