use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};

use openssl::x509::X509;
use pingora::prelude::HttpPeer;

use crate::core::error::{AppFail, AppResult};
use crate::module::config::{Balance, PoolConfig};
use super::{Backend, Lease, Pool};

impl Pool {

    pub fn new ( name: String, config: PoolConfig, observe: bool ) -> AppResult<Self> {

        let available=Arc::new(super::arch::Availability::default());
        let mut backends = Vec::new();
        for settings in &config.backends {
            let mut peer = HttpPeer::new(settings.address, settings.tls, settings.server_name.clone());
            peer.options.verify_cert = true;
            peer.options.verify_hostname = true;
            if let Some(path) = &settings.ca_file {
                let pem = std::fs::read(path).or_fail("Cannot read upstream CA file")?;
                let certificates = X509::stack_from_pem(&pem).or_fail("Invalid upstream CA certificates")?;
                if certificates.is_empty() { return Err(crate::core::error::AppError::invalid("Empty upstream CA file")); }
                peer.options.ca = Some(Arc::new(certificates.into_boxed_slice()));
            }
            backends.push(Arc::new(Backend {
                available: available.clone(),
                authority: http::HeaderValue::from_str(&settings.address.to_string()).or_fail("Invalid backend authority")?,
                config: settings.clone(), peer, probe: super::probe::Probe::new(settings, &config.options)?, active: AtomicU64::new(0), latency_us: AtomicU64::new(50000),
                failures: AtomicU32::new(0), down_until: AtomicU64::new(0), alive: AtomicBool::new(true),
                next_probe: AtomicU64::new(0),
            }));
        }

        let measure_latency = observe || config.options.policy == Balance::Adaptive;
        let track_active = observe || matches!(config.options.policy, Balance::Adaptive | Balance::LeastConn)
            || config.backends.iter().any(|backend| backend.max_in_flight > 0);
        Ok(Self { available, name, config, backends, measure_latency, track_active, cursor: AtomicU64::new(0), gate: Mutex::new(()) })

    }

    pub async fn wait_select ( &self, started: std::time::Instant, exclude: &[usize] ) -> Option<Lease> {
        if (0..self.backends.len()).all(|index|exclude.contains(&index)) {return None;}
        struct Waiting<'a>(&'a super::arch::Availability);
        impl Drop for Waiting<'_> { fn drop(&mut self) {self.0.waiters.fetch_sub(1,Ordering::SeqCst);} }
        self.available.waiters.fetch_add(1,Ordering::SeqCst);
        let _waiting=Waiting(&self.available);
        loop {
            let notified=self.available.notify.notified();
            tokio::pin!(notified);
            notified.as_mut().enable();
            let now=started.elapsed().as_millis() as u64;
            if let Some(lease)=self.select(now,exclude) {return Some(lease);}
            let cooldown=self.backends.iter().enumerate().filter(|(index,backend)| {
                !exclude.contains(index) && backend.alive.load(Ordering::Relaxed)
            }).map(|(_,backend)|backend.down_until.load(Ordering::Relaxed)).filter(|until|*until>now).min();
            // Capacity and active-health changes notify waiters. Only passive cooldown
            // requires a timer; avoid polling every waiting connection during outages.
            if let Some(until)=cooldown {
                tokio::select! { _ = notified => {}, _ = tokio::time::sleep(std::time::Duration::from_millis(until-now)) => {} }
            } else {notified.await; }
        }
    }

    pub fn select ( &self, now_ms: u64, exclude: &[usize] ) -> Option<Lease> {

        if self.backends.len() == 1 {
            let backend = &self.backends[0];
            if exclude.contains(&0) || !backend.available(now_ms) { return None; }
            if backend.config.max_in_flight == 0 {
                if self.track_active { backend.active.fetch_add(1, Ordering::Relaxed); }
            }
            else {
                backend.active.fetch_update(Ordering::Relaxed, Ordering::Relaxed, |active| {
                    (active < backend.config.max_in_flight).then_some(active + 1)
                }).ok()?;
            }
            return Some(Lease { backend: backend.clone(), index: 0, tracked: self.track_active, max_fails: self.config.options.max_fails, cooldown_ms: self.config.options.cooldown_ms });
        }
        let _guard = self.gate.lock().unwrap_or_else(|error| error.into_inner());
        let eligible = |(index, backend): &(usize, &Arc<Backend>)| {
            !exclude.contains(index) && backend.available(now_ms)
                && (backend.config.max_in_flight == 0 || backend.active.load(Ordering::Relaxed) < backend.config.max_in_flight)
        };
        let ticket = self.cursor.fetch_add(1, Ordering::Relaxed);
        let candidates = self.backends.iter().enumerate().filter(eligible);
        let selected = match self.config.options.policy {
            Balance::First => candidates.clone().next()?,
            Balance::RoundRobin => {
                let total:u64=candidates.clone().map(|(_,backend)|u64::from(backend.config.weight)).sum();
                if total==0 {return None;}
                let mut position=ticket%total;
                candidates.clone().find(|(_,backend)| {
                    let weight=u64::from(backend.config.weight);
                    if position<weight {true} else {position-=weight;false}
                })?
            }
            Balance::LeastConn | Balance::Adaptive => {
                let offset=ticket as usize%self.backends.len();
                candidates.min_by_key(|(index,backend)| {
                    let latency=if self.config.options.policy==Balance::Adaptive {backend.latency_us.load(Ordering::Relaxed).max(1)} else {1000};
                    (latency.saturating_mul(backend.active.load(Ordering::Relaxed)+1)/u64::from(backend.config.weight),
                     (index+self.backends.len()-offset)%self.backends.len())
                })?
            }
        };
        if self.track_active { selected.1.active.fetch_add(1, Ordering::Relaxed); }

        Some(Lease { backend: selected.1.clone(), index: selected.0, tracked: self.track_active, max_fails: self.config.options.max_fails, cooldown_ms: self.config.options.cooldown_ms })

    }

}

impl Drop for Lease {

    fn drop ( &mut self ) {

        if !self.tracked { return; }
        self.backend.active.fetch_sub(1, Ordering::Relaxed);
        if self.backend.available.waiters.load(Ordering::SeqCst)>0 {self.backend.available.notify.notify_one();}

    }

}

#[cfg(test)]
mod tests {

    use super::*;
    use crate::module::config::{BackendConfig, PoolOptions};

    fn pool ( policy: Balance ) -> Pool {
        Pool::new("test".into(), PoolConfig {
            backends: vec![
                BackendConfig { address: "127.0.0.1:3001".parse().unwrap(), max_in_flight: 1, ..BackendConfig::default() },
                BackendConfig { address: "127.0.0.1:3002".parse().unwrap(), ..BackendConfig::default() },
            ],
            options: PoolOptions { policy, max_fails: 1, cooldown_ms: 100, ..PoolOptions::default() },
        }, true).unwrap()
    }

    #[test]
    fn waiting_observes_probe_recovery_and_passive_cooldown_without_polling () {
        tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap().block_on(async {
            let pool=Arc::new(pool(Balance::First));
            let started=std::time::Instant::now();
            pool.backends[0].probe_result(false);
            let waiting=pool.clone();
            let task=tokio::spawn(async move {waiting.wait_select(started,&[1]).await});
            tokio::task::yield_now().await;
            assert!(!task.is_finished());
            pool.backends[0].probe_result(true);
            let lease=tokio::time::timeout(std::time::Duration::from_millis(250),task).await.unwrap().unwrap().unwrap();
            assert_eq!(lease.index,0);
            lease.finish(started.elapsed().as_millis() as u64,true);
            drop(lease);
            let lease=tokio::time::timeout(std::time::Duration::from_millis(500),pool.wait_select(started,&[1])).await.unwrap().unwrap();
            assert_eq!(lease.index,0);
            assert!(started.elapsed().as_millis()>=100);
            assert!(pool.wait_select(started,&[0,1]).await.is_none());
        });
    }

    #[test]
    fn adaptive_selection_uses_latency_load_and_explicit_capacity () {
        let pool = pool(Balance::Adaptive);
        pool.backends[0].latency_us.store(1000, Ordering::Relaxed);
        pool.backends[1].latency_us.store(10000, Ordering::Relaxed);
        let first = pool.select(0, &[]).unwrap();
        assert_eq!(first.index, 0);
        assert_eq!(pool.select(0, &[]).unwrap().index, 1);
        drop(first);
        assert_eq!(pool.select(0, &[]).unwrap().index, 0);
        pool.backends[0].latency_us.store(100000, Ordering::Relaxed);
        assert_eq!(pool.select(0, &[]).unwrap().index, 1);
    }

    #[test]
    fn quarantine_exclusion_and_raii_release_are_effective () {
        let pool = pool(Balance::First);
        let first = pool.select(0, &[]).unwrap();
        first.finish(0, true);
        drop(first);
        assert_eq!(pool.select(1, &[]).unwrap().index, 1);
        assert_eq!(pool.select(101, &[]).unwrap().index, 0);
        assert_eq!(pool.select(101, &[0]).unwrap().index, 1);
        assert!(pool.select(101, &[0, 1]).is_none());
        assert_eq!(pool.backends[0].active.load(Ordering::Relaxed), 0);
    }

}
