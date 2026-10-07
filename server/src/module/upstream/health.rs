use std::sync::atomic::Ordering;

use super::{Backend, Lease};

impl Backend {

    pub fn available ( &self, now_ms: u64 ) -> bool {

        self.alive.load(Ordering::Relaxed) && now_ms >= self.down_until.load(Ordering::Relaxed)

    }

    pub fn probe_result ( &self, healthy: bool ) -> bool {

        let changed=self.alive.swap(healthy, Ordering::Relaxed) != healthy;
        if changed && healthy {self.available.notify.notify_waiters();}
        changed

    }

}

impl Lease {

    pub fn latency ( &self, micros: u64 ) {

        let sample = micros.clamp(1, 120000000);
        let _ = self.backend.latency_us.fetch_update(Ordering::Relaxed, Ordering::Relaxed, |old| Some((old * 7 + sample) / 8));

    }

    pub fn finish ( &self, now_ms: u64, failed: bool ) {

        if failed {
            let failures = self.backend.failures.fetch_add(1, Ordering::Relaxed) + 1;
            if failures >= self.max_fails {
                self.backend.down_until.store(now_ms.saturating_add(self.cooldown_ms), Ordering::Relaxed);
                self.backend.failures.store(0, Ordering::Relaxed);
            }
        } else if self.backend.failures.load(Ordering::Relaxed) != 0 {
            self.backend.failures.store(0, Ordering::Relaxed);
        }

    }

}

impl super::Pool {
    pub fn stats ( &self, now_ms: u64 ) -> serde_json::Value {
        serde_json::json!({"name": self.name, "policy": format!("{:?}", self.config.options.policy),
            "backends": self.backends.iter().map(|backend| serde_json::json!({
                "address": backend.config.address, "healthy": backend.available(now_ms),
                "active": backend.active.load(Ordering::Relaxed),
                "latency_us": backend.latency_us.load(Ordering::Relaxed), "weight": backend.config.weight,
            })).collect::<Vec<_>>()})
    }
}
