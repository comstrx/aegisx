use std::sync::atomic::{AtomicU64, Ordering};

static NEXT: AtomicU64 = AtomicU64::new(1);
thread_local! { static GROUP: u64 = NEXT.fetch_add(1, Ordering::Relaxed); }

/// Keep idle sockets on the runtime that owns their I/O reactor.
/// Work-stealing runtimes retain shared reuse because tasks may migrate.
pub fn group ( work_stealing: bool ) -> u64 {
    if work_stealing { 0 } else { GROUP.with(|value| *value) }
}

#[cfg(test)]
mod tests {
    #[test]
    fn groups_are_stable_per_worker_and_shared_when_tasks_can_migrate () {
        let local=super::group(false);
        assert_eq!(local,super::group(false));
        assert_eq!(super::group(true),0);
        let other=std::thread::spawn(||super::group(false)).join().unwrap();
        assert_ne!(local,other);
        assert_ne!(other,0);
    }
}
