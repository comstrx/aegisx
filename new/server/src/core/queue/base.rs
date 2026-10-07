use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, TrySendError, sync_channel};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crate::core::error::{AppError, AppFail, AppResult};
use super::arch::{Job, Queue, Spec, Stats, Token};

impl <J: Send + 'static> Queue <J> {

    pub fn start <H> ( spec: Spec, handler: H ) -> AppResult<Self>
    where H: Fn(J, &Token) -> AppResult<()> + Send + Sync + 'static {

        if spec.workers == 0 || spec.capacity == 0 { return Err(AppError::invalid("queue", format!("{} needs at least one worker and one slot", spec.name))); }

        let ( sender, receiver ) = sync_channel::<Job<J>>(spec.capacity);
        let receiver = Arc::new(Mutex::new(receiver));
        let stats = Arc::new(Stats::default());
        let stopping = Arc::new(AtomicBool::new(false));
        let handler = Arc::new(handler);
        let mut workers = Vec::with_capacity(spec.workers);

        for index in 0..spec.workers {

            let receiver = receiver.clone();
            let stats = stats.clone();
            let stopping = stopping.clone();
            let handler = handler.clone();

            let worker = std::thread::Builder::new().name(format!("{}-{index}", spec.name)).spawn(move || Self::serve(&receiver, &stats, &stopping, &*handler))
                .or_fail_with(|| format!("cannot spawn {} worker {index}", spec.name))?;

            workers.push(worker);

        }

        Ok(Self { spec, sender: Mutex::new(Some(sender)), stats, stopping, workers: Mutex::new(workers) })

    }

    pub fn submit ( &self, payload: J ) -> Result<(), J> {

        let now = Instant::now();
        let job = Job { payload, submitted: now, deadline: now + Duration::from_millis(self.spec.deadline_ms) };

        let slot = self.sender.lock().unwrap_or_else(|poisoned| poisoned.into_inner());

        let Some(sender) = slot.as_ref() else { self.stats.rejected.fetch_add(1, Ordering::Relaxed); return Err(job.payload); };

        match sender.try_send(job) {
            Ok(()) => { self.stats.submitted.fetch_add(1, Ordering::Relaxed); Ok(()) }
            Err(TrySendError::Full(job) | TrySendError::Disconnected(job)) => { self.stats.rejected.fetch_add(1, Ordering::Relaxed); Err(job.payload) }
        }

    }

    pub fn stats ( &self ) -> &Stats {

        &self.stats

    }

    pub fn spec ( &self ) -> Spec {

        self.spec

    }

    pub fn queued ( &self ) -> u64 {

        let submitted = self.stats.submitted.load(Ordering::Relaxed);
        let done = self.stats.finished.load(Ordering::Relaxed) + self.stats.failed.load(Ordering::Relaxed) + self.stats.expired.load(Ordering::Relaxed);

        submitted.saturating_sub(done)

    }

    pub fn stop ( &self ) {

        self.stopping.store(true, Ordering::Release);

        if let Ok(mut slot) = self.sender.lock() { slot.take(); }

        let workers = self.workers.lock().map(|mut workers| std::mem::take(&mut *workers)).unwrap_or_default();

        for worker in workers { let _ = worker.join(); }

    }

    fn serve <H> ( receiver: &Mutex<Receiver<Job<J>>>, stats: &Stats, stopping: &Arc<AtomicBool>, handler: &H )
    where H: Fn(J, &Token) -> AppResult<()> {

        loop {

            let job = match receiver.lock() {
                Ok(receiver) => receiver.recv(),
                Err(_) => return,
            };

            let Ok(job) = job else { return; };

            if stopping.load(Ordering::Acquire) { stats.expired.fetch_add(1, Ordering::Relaxed); continue; }

            let started = Instant::now();

            if started >= job.deadline { stats.expired.fetch_add(1, Ordering::Relaxed); continue; }

            stats.active.fetch_add(1, Ordering::Relaxed);

            let token = Token { stopping: stopping.clone(), deadline: job.deadline };
            let outcome = handler(job.payload, &token);
            let elapsed = started.elapsed().as_micros() as u64;

            stats.active.fetch_sub(1, Ordering::Relaxed);
            stats.wait_us.fetch_add(started.duration_since(job.submitted).as_micros() as u64, Ordering::Relaxed);
            stats.total_us.fetch_add(elapsed, Ordering::Relaxed);
            stats.last_us.store(elapsed, Ordering::Relaxed);

            match outcome {
                Ok(()) => stats.finished.fetch_add(1, Ordering::Relaxed),
                Err(_) => stats.failed.fetch_add(1, Ordering::Relaxed),
            };

        }

    }

}

impl Token {

    pub fn cancelled ( &self ) -> bool {

        self.stopping.load(Ordering::Acquire) || Instant::now() >= self.deadline

    }

    pub fn remaining_ms ( &self ) -> u64 {

        self.deadline.saturating_duration_since(Instant::now()).as_millis() as u64

    }

}

impl <J> Drop for Queue <J> {

    fn drop ( &mut self ) {

        self.stopping.store(true, Ordering::Release);

        if let Ok(mut slot) = self.sender.lock() { slot.take(); }

    }

}
