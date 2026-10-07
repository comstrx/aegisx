use std::collections::VecDeque;
use std::sync::{Arc, Mutex, atomic::{AtomicU64, Ordering}};
use serde_json::json;

use crate::module::{config::TelemetryConfig, storage::Event};

pub struct Telemetry {
    total: AtomicU64,
    samples: AtomicU64,
    active: AtomicU64,
    completed: AtomicU64,
    blocked: AtomicU64,
    failed: AtomicU64,
    recent: Mutex<VecDeque<Event>>,
    capacity: usize,
    latency: [AtomicU64;8],
    request_bytes: AtomicU64,
    response_bytes: AtomicU64,
}

pub struct Ticket { telemetry: Arc<Telemetry>, done: bool }

impl Telemetry {

    pub fn new ( capacity: usize ) -> Self {
        Self { samples: AtomicU64::new(0), latency: std::array::from_fn(|_|AtomicU64::new(0)), request_bytes: AtomicU64::new(0), response_bytes: AtomicU64::new(0), total: AtomicU64::new(0), active: AtomicU64::new(0), completed: AtomicU64::new(0),
            blocked: AtomicU64::new(0), failed: AtomicU64::new(0), recent: Mutex::new(VecDeque::new()), capacity }
    }

    pub fn begin ( self: &Arc<Self>, config: &TelemetryConfig, control: bool ) -> (Option<Ticket>, bool) {
        if !config.enabled || !control {
            let sampled=config.sample_every==1 || self.samples.fetch_add(1,Ordering::Relaxed).is_multiple_of(config.sample_every);
            return (None,sampled);
        }
        let count = self.total.fetch_add(1, Ordering::Relaxed);
        self.active.fetch_add(1, Ordering::Relaxed);
        (Some(Ticket { telemetry: self.clone(), done: false }), count.is_multiple_of(config.sample_every))
    }

    pub fn publish ( &self, event: Event ) {
        let mut recent = self.recent.lock().unwrap_or_else(|error| error.into_inner());
        while recent.len() >= self.capacity { recent.pop_front(); }
        recent.push_back(event);
    }

    pub fn observe ( &self, elapsed_ms: u64, request: u64, response: u64 ) {
        let bucket=[1,5,10,25,50,100,500].iter().position(|limit|elapsed_ms<=*limit).unwrap_or(7);
        self.latency[bucket].fetch_add(1,Ordering::Relaxed);
        self.request_bytes.fetch_add(request,Ordering::Relaxed);
        self.response_bytes.fetch_add(response,Ordering::Relaxed);
    }

    pub fn snapshot ( &self ) -> serde_json::Value {
        json!({
            "latency_buckets":self.latency.iter().map(|value|value.load(Ordering::Relaxed)).collect::<Vec<_>>(),
            "request_bytes":self.request_bytes.load(Ordering::Relaxed),"response_bytes":self.response_bytes.load(Ordering::Relaxed),
            "total": self.total.load(Ordering::Relaxed), "active": self.active.load(Ordering::Relaxed),
            "completed": self.completed.load(Ordering::Relaxed), "blocked": self.blocked.load(Ordering::Relaxed),
            "failed": self.failed.load(Ordering::Relaxed),
            "recent": self.recent.lock().unwrap_or_else(|error| error.into_inner()).iter().rev().take(100).collect::<Vec<_>>(),
        })
    }

}

impl Ticket {
    pub fn finish ( &mut self, failed: bool, blocked: bool ) {
        if self.done { return; }
        self.done = true;
        self.telemetry.active.fetch_sub(1, Ordering::Relaxed);
        let counter = if blocked { &self.telemetry.blocked } else if failed { &self.telemetry.failed } else { &self.telemetry.completed };
        counter.fetch_add(1, Ordering::Relaxed);
    }
}

impl Drop for Ticket {
    fn drop ( &mut self ) { self.finish(true, false); }
}


#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn sampling_remains_independent_of_dashboard_metrics () {
        let telemetry=Arc::new(Telemetry::new(8));
        let config=TelemetryConfig {sample_every:2,..Default::default()};
        assert!(telemetry.begin(&config,false).1);
        assert!(!telemetry.begin(&config,false).1);
        assert_eq!(telemetry.snapshot()["total"],0);
        let (ticket,_) = telemetry.begin(&config,true);
        assert_eq!(telemetry.snapshot()["active"],1);
        drop(ticket);
        assert_eq!(telemetry.snapshot()["active"],0);
        assert_eq!(telemetry.snapshot()["failed"],1);
    }
}
