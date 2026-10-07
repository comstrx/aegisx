use std::collections::HashMap;
use std::hash::{BuildHasher, RandomState};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::time::Instant;

use crate::config::{BackendConfig, Balance, Config};
use crate::core::error::{AppError, AppResult};
use crate::core::net::Address;
use crate::http::upstream::Upstream;
use crate::http::tls::{ALPN_HTTP1, ALPN_HTTP2, Tls};
use crate::http::upstream::Protocol;
use super::arch::{Backend, Counts, HashKey, Lease, Picker, PoolState, Pools, Resolved, Retry, Sticky};

impl Pools {

    pub fn build ( config: &Config, resolved: &Resolved ) -> AppResult<Self> {

        Self::assemble(config, None, resolved)

    }

    pub fn named ( backend: &BackendConfig ) -> Option<&str> {

        backend.srv.as_deref().or(backend.address.name().map(|( host, _ )| host))

    }

    pub fn rebuild ( config: &Config, previous: &Pools, resolved: &Resolved ) -> AppResult<Self> {

        Self::assemble(config, Some(previous), resolved)

    }

    fn assemble ( config: &Config, previous: Option<&Pools>, resolved: &Resolved ) -> AppResult<Self> {

        let started = previous.map_or_else(Instant::now, |pools| pools.started);
        let mut carried: HashMap<( &str, Address ), &Arc<Backend>> = HashMap::new();

        if let Some(previous) = previous {

            for pool in &previous.list {

                for backend in &pool.backends { carried.insert(( &pool.name, backend.addr.clone() ), backend); }

            }

        }

        let mut list = Vec::with_capacity(config.pools.len());
        let workers = config.worker_count().max(1);

        for ( index, ( name, spec ) ) in config.pools.iter().enumerate() {

            let mut backends = Vec::with_capacity(spec.backends.len());
            let mut expanded: Vec<( Address, Option<Arc<str>> )> = Vec::with_capacity(spec.backends.len());

            for backend in &spec.backends {

                match Self::named(backend) {
                    Some(host) => {

                        let addresses = resolved.get(host).filter(|addresses| !addresses.is_empty()).ok_or_else(|| AppError::config("add_upstream", format!("pool `{name}` backend {host} did not resolve to any address")))?;

                        expanded.extend(addresses.iter().map(|addr| ( Address::Tcp(*addr), Some(Arc::<str>::from(host)) )));

                    }
                    None => expanded.push(( backend.address.clone(), None )),
                }

            }

            let specs = spec.backends.iter().flat_map(|backend| std::iter::repeat_n(backend, Self::named(backend).map_or(1, |host| resolved.get(host).map_or(0, Vec::len))));

            for ( position, ( backend, ( address, origin ) ) ) in specs.zip(expanded).enumerate() {

                let old = carried.get(&( name.as_str(), address.clone() ));

                let protocols: &[&[u8]] = match backend.protocol {
                    Protocol::Auto => &[ALPN_HTTP2, ALPN_HTTP1],
                    Protocol::Http1 | Protocol::Fastcgi => &[ALPN_HTTP1],
                    Protocol::Http2 => &[ALPN_HTTP2],
                };

                let server_name = if backend.server_name.is_empty() { origin.as_deref().unwrap_or("") } else { backend.server_name.as_str() };
                let upstream = match backend.tls {
                    true => Upstream::secure(address.clone(), Tls::trust(server_name, backend.ca_file.as_deref(), backend.cert.as_deref().zip(backend.key.as_deref()), protocols)?, backend.protocol),
                    false => Upstream::new(address.clone(), backend.protocol),
                };

                backends.push(Arc::new(Backend {
                    index      : position,
                    id         : Backend::identity(name, &address.to_string()),
                    addr       : address,
                    origin,
                    upstream,
                    weight     : backend.weight.max(1),
                    limit      : backend.max_in_flight,
                    backup     : backend.backup,
                    down       : backend.down,
                    active     : AtomicU64::new(0),
                    fails      : AtomicU32::new(old.map_or(0, |old| old.fails.load(Ordering::Relaxed))),
                    down_until : AtomicU64::new(old.map_or(0, |old| old.down_until.load(Ordering::Relaxed))),
                    revived    : AtomicU64::new(match ( old, previous ) { ( Some(old), _ ) => old.revived.load(Ordering::Relaxed), ( None, Some(previous) ) => previous.now_ms().max(1), ( None, None ) => 0 }),
                    probed     : AtomicBool::new(old.is_none_or(|old| old.probed.load(Ordering::Relaxed))),
                    latency_us : AtomicU64::new(old.map_or(0, |old| old.latency_us.load(Ordering::Relaxed))),
                    counts     : old.filter(|old| old.counts.len() == workers).map_or_else(|| (0..workers).map(|_| Counts::default()).collect(), |old| old.counts.clone()),
                }));

            }

            let counting = matches!(spec.options.policy, Balance::LeastConn | Balance::LeastTime | Balance::Random) || spec.backends.iter().any(|backend| backend.max_in_flight > 0);
            let hash = match spec.options.policy {
                Balance::IpHash => Some(HashKey::compile(spec.options.hash_key.as_deref(), HashKey::Ip)?),
                Balance::Hash => Some(HashKey::compile(spec.options.hash_key.as_deref(), HashKey::Uri)?),
                _ => None,
            };

            list.push(Arc::new(PoolState {
                index,
                name        : Arc::from(name.as_str()),
                backends,
                policy      : spec.options.policy,
                max_fails   : spec.options.max_fails,
                cooldown_ms : spec.options.cooldown_ms,
                slow_start  : spec.options.slow_start_ms,
                max_ejected : spec.options.max_ejected,
                attempts    : spec.options.attempts,
                keepalive   : spec.options.keepalive,
                slow_us     : spec.options.slow_ms.saturating_mul(1_000),
                retry       : Retry::compile(&spec.options.retry_on, spec.options.retry_non_idempotent),
                counting,
                health      : spec.options.health.clone(),
                expect      : spec.options.health.as_ref().and_then(|health| health.body.as_deref()).map(|body| regex::bytes::Regex::new(&body.strip_prefix('~').map_or_else(|| regex::escape(body), str::to_owned)).map_err(|error| AppError::config("set_balancer", format!("pool `{name}` health body: {error}")))).transpose()?,
                hash,
                sticky      : spec.options.sticky.as_ref().map(Sticky::compile),
                spare       : spec.backends.iter().any(|backend| backend.backup && !backend.down),
            }));

        }

        Ok(Self { list, started })

    }

    pub fn now_ms ( &self ) -> u64 {

        self.started.elapsed().as_millis() as u64

    }

    pub fn since ( &self, at: Instant ) -> u64 {

        at.saturating_duration_since(self.started).as_millis() as u64

    }

    pub fn get ( &self, index: usize ) -> Option<&Arc<PoolState>> {

        self.list.get(index)

    }

    pub fn find ( &self, name: &str ) -> Option<&Arc<PoolState>> {

        self.list.iter().find(|pool| &*pool.name == name)

    }

}

impl Picker {

    pub fn new ( pools: &Pools ) -> Self {

        Self {
            cursors : vec![0; pools.list.len()],
            current : pools.list.iter().map(|pool| vec![0; pool.backends.len()]).collect(),
            latency : pools.list.iter().map(|pool| vec![0; pool.backends.len()]).collect(),
            samples : pools.list.iter().map(|pool| vec![0; pool.backends.len()]).collect(),
            seed    : RandomState::new().hash_one(pools.list.len()) | 1,
        }

    }

    pub fn fit ( &mut self, pools: &Pools ) {

        if self.current.len() != pools.list.len() || self.current.iter().zip(&pools.list).any(|( state, pool )| state.len() != pool.backends.len()) {

            *self = Self::new(pools);

        }

    }

}

impl Lease {

    pub fn new ( backend: Arc<Backend> ) -> Self {

        backend.active.fetch_add(1, Ordering::Relaxed);

        Self { backend }

    }

}

impl Drop for Lease {

    fn drop ( &mut self ) {

        self.backend.active.fetch_sub(1, Ordering::Relaxed);

    }

}

impl Retry {

    pub fn compile ( tokens: &[String], non_idempotent: bool ) -> Self {

        let mut retry = Self { non_idempotent, ..Self::default() };

        for token in tokens {

            match token.as_str() {
                "connect" => retry.connect = true,
                "error" => retry.error = true,
                "timeout" => retry.timeout = true,
                "5xx" => retry.server_errors = true,
                code => { if let Ok(status) = code.parse::<u16>() { retry.statuses.push(status); } }
            }

        }

        retry

    }

    pub fn replays ( &self ) -> bool {

        self.error || self.timeout || self.server_errors || !self.statuses.is_empty()

    }

    pub fn rerun ( &self ) -> bool {

        self.server_errors || !self.statuses.is_empty()

    }

    pub fn status ( &self, status: u16 ) -> bool {

        self.statuses.contains(&status) || (self.server_errors && status >= 500)

    }

    pub fn failure ( &self, connect: bool, timeout: bool ) -> bool {

        if connect { return self.connect; }

        if timeout { return self.timeout; }

        self.error

    }

}
