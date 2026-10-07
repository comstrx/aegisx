use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::thread;
use std::time::Duration;

use aegisx::core::queue::{Queue, Spec};

fn spec ( workers: usize, capacity: usize, deadline_ms: u64 ) -> Spec {

    Spec { name: "test-queue", workers, capacity, deadline_ms }

}

#[test]
fn jobs_run_on_workers_and_stats_follow () {

    let seen = Arc::new(AtomicU64::new(0));
    let counter = seen.clone();

    let queue = Queue::start(spec(2, 64, 5_000), move |job: u64, token| {

        assert!(!token.cancelled());
        counter.fetch_add(job, Ordering::SeqCst);

        if job == 13 { Err(aegisx::core::error::AppError::message("unlucky")) } else { Ok(()) }

    }).expect("queue");

    for job in 1..=20u64 { assert!(queue.submit(job).is_ok()); }

    for _ in 0..200 {

        if queue.queued() == 0 { break; }

        thread::sleep(Duration::from_millis(10));

    }

    assert_eq!(seen.load(Ordering::SeqCst), 210);
    assert_eq!(queue.stats().submitted.load(Ordering::Relaxed), 20);
    assert_eq!(queue.stats().finished.load(Ordering::Relaxed), 19);
    assert_eq!(queue.stats().failed.load(Ordering::Relaxed), 1);
    assert_eq!(queue.stats().active.load(Ordering::Relaxed), 0);

    queue.stop();

    assert_eq!(queue.submit(99), Err(99));

}

#[test]
fn full_queues_reject_without_blocking () {

    let queue = Queue::start(spec(1, 2, 5_000), |_: u64, _| { thread::sleep(Duration::from_millis(150)); Ok(()) }).expect("queue");

    thread::sleep(Duration::from_millis(20));

    assert!(queue.submit(1).is_ok());

    thread::sleep(Duration::from_millis(20));

    assert!(queue.submit(2).is_ok());
    assert!(queue.submit(3).is_ok());
    assert_eq!(queue.submit(4), Err(4));
    assert_eq!(queue.stats().rejected.load(Ordering::Relaxed), 1);

    queue.stop();

}

#[test]
fn expired_jobs_are_skipped_and_tokens_report_cancellation () {

    let ran = Arc::new(AtomicU64::new(0));
    let counter = ran.clone();

    let queue = Queue::start(spec(1, 8, 60), move |job: u64, token| {

        counter.fetch_add(1, Ordering::SeqCst);

        if job == 1 { thread::sleep(Duration::from_millis(120)); assert!(token.cancelled()); assert_eq!(token.remaining_ms(), 0); }

        Ok(())

    }).expect("queue");

    assert!(queue.submit(1).is_ok());
    assert!(queue.submit(2).is_ok());

    thread::sleep(Duration::from_millis(300));

    assert_eq!(ran.load(Ordering::SeqCst), 1);
    assert_eq!(queue.stats().expired.load(Ordering::Relaxed), 1);
    assert_eq!(queue.queued(), 0);

    queue.stop();

}

#[test]
fn start_rejects_empty_specs () {

    assert!(Queue::<u8>::start(spec(0, 1, 10), |_, _| Ok(())).is_err());
    assert!(Queue::<u8>::start(spec(1, 0, 10), |_, _| Ok(())).is_err());

}
