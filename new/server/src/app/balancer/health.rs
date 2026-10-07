use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::Duration;

use crate::app::State;
use crate::config::HealthConfig;
use crate::core::log::{debug, info, warn};
use crate::core::rt::Rt;
use crate::core::sync::Watch;
use crate::http::upstream::Client;
use super::arch::{Backend, PoolState, Pools};

impl Pools {

    pub async fn probe_loop ( state: State, mut stop: Watch ) {

        let mut tick = 0u64;
        let client = Client::new(state.load().snapshot.config.client_settings());

        loop {

            let runtime = state.load();

            if tick.is_multiple_of(10_000) { state.memory().sweep(runtime.pools.now_ms()); }

            for pool in &runtime.pools.list {

                let Some(health) = &pool.health else { continue; };

                if !tick.is_multiple_of(health.interval_ms.max(100)) { continue; }

                for backend in pool.backends.iter().filter(|backend| !backend.down) {

                    let healthy = Self::probe(&client, backend, health, pool.expect.as_ref()).await;
                    let before = backend.probed.swap(healthy, Ordering::Relaxed);

                    if before != healthy {

                        if healthy { backend.revived.store(runtime.pools.now_ms().max(1), Ordering::Relaxed); info!(pool = %pool.name, backend = %backend.addr, "backend healthy"); }
                        else { warn!(pool = %pool.name, backend = %backend.addr, "backend unhealthy"); }

                    }

                }

            }

            tokio::select! {
                _ = Rt::sleep(100) => tick += 100,
                _ = stop.wait() => break,
            }

        }

    }

    async fn probe ( client: &Client, backend: &Arc<Backend>, health: &HealthConfig, expect: Option<&regex::bytes::Regex> ) -> bool {

        let attempt = async {

            match client.probe(&backend.upstream, health.path.as_deref(), health.timeout_ms).await.ok()? {
                None => Some(true),
                Some(( status, body )) => Some(status == health.status && expect.is_none_or(|pattern| pattern.is_match(&body))),
            }

        };

        match tokio::time::timeout(Duration::from_millis(health.timeout_ms), attempt).await {
            Ok(Some(healthy)) => healthy,
            Ok(None) => false,
            Err(_) => { debug!(backend = %backend.addr, "health probe timed out"); false }
        }

    }

    pub fn describe ( pool: &PoolState ) -> String {

        format!("{} ({} backends, {:?})", pool.name, pool.backends.len(), pool.policy)

    }

}
