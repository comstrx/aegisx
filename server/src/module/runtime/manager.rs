use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::thread;
use std::time::{Duration, Instant};

use arc_swap::ArcSwap;
use tokio::sync::oneshot;

use crate::core::error::{AppError, AppFail, AppResult};
use crate::module::config::Config;
use crate::module::inference::{Model, ModelInfo};
use super::{Manager, ManagerGuard, Snapshot};

impl Manager {

    pub fn start ( path: PathBuf, current: Arc<ArcSwap<Snapshot>>, info: Option<ModelInfo>, started: Instant ) -> AppResult<ManagerGuard> {

        let runtime = tokio::runtime::Builder::new_current_thread().enable_all().build().or_fail("Cannot create maintenance runtime")?;
        let (sender, mut stop) = oneshot::channel();
        let (ready_tx, ready_rx) = std::sync::mpsc::sync_channel(1);
        let thread = thread::Builder::new().name("aegisx-maintain".into()).spawn(move || runtime.block_on(async move {
            let mut signal = match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::hangup()) {
                Ok(signal) => signal,
                Err(error) => { let _ = ready_tx.send(Err(error.to_string())); return; }
            };
            let _ = ready_tx.send(Ok(()));
            let mut timer = tokio::time::interval(Duration::from_millis(100));
            timer.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            loop {
                tokio::select! {
                    _ = &mut stop => break,
                    _ = signal.recv() => {
                        let previous = current.load_full();
                        let result = Config::load(&path).and_then(|config| {
                            previous.compatible(&config, info.is_some())?;
                            Model::validate_policy(&config, info.as_ref())?;
                            Snapshot::build(config, Some(&previous))
                        });
                        match result {
                            Ok(snapshot) => {
                                tracing::info!(config_version = %snapshot.version, "Configuration reloaded");
                                current.store(Arc::new(snapshot));
                            }
                            Err(error) => tracing::warn!(error = %error, "Reload rejected; previous configuration remains active"),
                        }
                    }
                    _ = timer.tick() => {
                        let snapshot = current.load_full();
                        let now = started.elapsed().as_millis() as u64;
                        let mut probes = tokio::task::JoinSet::new();
                        for pool in snapshot.pools.values() {
                            let options = &pool.config.options;
                            if options.health_interval_ms == 0 { continue; }
                            for backend in &pool.backends {
                                if now < backend.next_probe.load(Ordering::Relaxed) { continue; }
                                backend.next_probe.store(now + options.health_interval_ms, Ordering::Relaxed);
                                let backend = backend.clone();
                                let name = pool.name.clone();
                                let timeout = Duration::from_millis(options.health_timeout_ms);
                                probes.spawn(async move {
                                    let healthy = backend.probe(timeout).await;
                                    if backend.probe_result(healthy) {
                                        tracing::info!(pool = %name, address = %backend.config.address, healthy, "Upstream health changed");
                                    }
                                });
                            }
                        }
                        while !probes.is_empty() {
                            tokio::select! {
                                _ = &mut stop => { probes.abort_all(); return; }
                                _ = probes.join_next() => {}
                            }
                        }
                    }
                }
            }
        })).or_fail("Cannot start maintenance worker")?;
        ready_rx.recv().or_fail("Maintenance worker failed to initialize")?
            .map_err(|message| AppError::invalid(format!("Cannot register reload signal: {message}")))?;

        Ok(ManagerGuard { stop: Some(sender), thread: Some(thread) })

    }

}

impl ManagerGuard {

    pub fn finish ( mut self ) -> AppResult<()> {

        if let Some(stop) = self.stop.take() { let _ = stop.send(()); }
        if let Some(thread) = self.thread.take() {
            thread.join().map_err(|_| AppError::invalid("Maintenance worker panicked"))?;
        }

        Ok(())

    }

}
