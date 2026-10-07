use std::sync::Arc;

use std::sync::Mutex;

use std::collections::HashMap;
use std::sync::RwLock;

use crate::app::{Acme, Decisions, Memory, Models, Pools, Resolved, Store, Summon};
use crate::config::AcmeChallenge;
use crate::config::base::consts::{ACTOR_IDLE_MS, ACTORS_MAX};
use crate::config::{BackendConfig, Config};
use super::arch::Snapshot;
use crate::core::error::{AppError, AppResult};
use crate::core::sync::{Lens, Swap};
use crate::http::files::FileCache;
use crate::config::ClientAuth;
use crate::http::tls::{Acceptor, Authority, Bundle, Challenges, ClientPolicy, Demand, Held, Obtain, Policy, Resumption, Tls};
use super::arch::Tokens;
use super::arch::{Overlay, Runtime, State};

impl Overlay {

    pub fn join ( &mut self, pool: &str, backend: BackendConfig ) {

        let key = ( pool.to_string(), backend.address.to_string() );

        self.left.remove(&key);
        self.joined.insert(key, backend);

    }

    pub fn leave ( &mut self, pool: &str, address: &str ) {

        let key = ( pool.to_string(), address.to_string() );

        self.joined.remove(&key);
        self.left.insert(key);

    }

    pub fn clear ( &mut self ) {

        self.joined.clear();
        self.left.clear();

    }

    pub fn len ( &self ) -> usize {

        self.joined.len() + self.left.len()

    }

    pub fn is_empty ( &self ) -> bool {

        self.len() == 0

    }

    fn apply ( &self, config: &mut Config ) {

        for ( ( pool, address ), backend ) in &self.joined {

            let Some(pool) = config.pools.get_mut(pool) else { continue; };

            match pool.backends.iter_mut().find(|existing| existing.address.to_string() == *address) {
                Some(existing) => *existing = backend.clone(),
                None => pool.backends.push(backend.clone()),
            }

        }

        for ( pool, address ) in &self.left {

            let Some(pool) = config.pools.get_mut(pool) else { continue; };

            if pool.backends.iter().any(|backend| backend.address.to_string() != *address) { pool.backends.retain(|backend| backend.address.to_string() != *address); }

        }

    }

}

impl State {

    pub fn new ( config: Config ) -> AppResult<Self> {

        let base = Arc::new(Mutex::new(config.clone()));
        let mut config = config;

        config.normalize()?;
        config.validate()?;

        let acme = config.tls.as_ref().and_then(|tls| tls.acme.as_ref());

        if let Some(acme) = acme { Acme::materialize(acme)?; }

        let challenges: Option<Challenges> = acme.map(|_| Arc::new(RwLock::new(HashMap::new())));
        let tokens: Option<Tokens> = acme.filter(|acme| acme.challenge == AcmeChallenge::Http01).map(|_| Arc::new(RwLock::new(HashMap::new())));
        let held: Held = Arc::new(RwLock::new(HashMap::new()));

        if let Some(acme) = acme.filter(|_| config.tls.as_ref().is_some_and(|tls| tls.on_demand.is_some())) { Summon::restore(acme, &held); }

        let tls = Self::acceptor(&config, challenges.clone(), tokens.clone(), held.clone())?;
        let resolved = Pools::resolve_blocking(&config)?;
        let pools = Pools::build(&config, &resolved)?;

        Models::install(&config)?;

        let decisions = config.decisions.enabled.then(|| Decisions::open(&config.decisions)).transpose()?.map(Arc::new);
        let key = decisions.as_ref().map(|decisions| decisions.identity_key()).transpose()?;
        let memory = Arc::new(Memory::new(ACTORS_MAX, ACTOR_IDLE_MS, key)?);
        let files = Arc::new(config.file_cache());
        let cache = config.cache.enabled.then(|| Arc::new(Store::open(&config.cache)));
        let snapshot = Snapshot::build(config, 1)?;

        Ok(Self { swap: Swap::new(Runtime { snapshot: Arc::new(snapshot), pools: Arc::new(pools) }), tls: Swap::new(tls), names: Arc::new(Mutex::new(resolved)), challenges, tokens, held, memory, decisions, files, cache, base, overlay: Arc::new(Mutex::new(Overlay::default())) })

    }

    pub fn load ( &self ) -> Arc<Runtime> {

        self.swap.load()

    }

    pub fn lens ( &self ) -> Lens<Runtime> {

        self.swap.lens()

    }

    pub fn memory ( &self ) -> Arc<Memory> {

        self.memory.clone()

    }

    pub fn cache ( &self ) -> Option<Arc<Store>> {

        self.cache.clone()

    }

    pub fn files ( &self ) -> Arc<FileCache> {

        self.files.clone()

    }

    pub fn decisions ( &self ) -> Option<Arc<Decisions>> {

        self.decisions.clone()

    }

    pub fn tls ( &self ) -> Swap<Option<Acceptor>> {

        self.tls.clone()

    }

    pub fn reload ( &self, config: Config ) -> AppResult<u64> {

        let overlay = self.overlay.lock().map_err(|_| AppError::config("reload", "the runtime overlay is unavailable"))?;
        let version = self.install(config.clone(), &overlay)?;

        if let Ok(mut base) = self.base.lock() { *base = config; }

        Ok(version)

    }

    pub fn adjust ( &self, change: impl FnOnce(&Config, &mut Overlay) -> Result<(), &'static str> ) -> Result<AppResult<u64>, &'static str> {

        let mut overlay = self.overlay.lock().map_err(|_| "state_unavailable")?;
        let mut next = overlay.clone();

        change(&self.swap.load().snapshot.config, &mut next)?;

        let base = self.base.lock().map_err(|_| "state_unavailable")?.clone();
        let outcome = self.install(base, &next);

        if outcome.is_ok() { *overlay = next; }

        Ok(outcome)

    }

    pub fn edits ( &self ) -> usize {

        self.overlay.lock().map_or(0, |overlay| overlay.len())

    }

    fn install ( &self, config: Config, overlay: &Overlay ) -> AppResult<u64> {

        let mut config = config;

        overlay.apply(&mut config);
        config.normalize()?;
        config.validate()?;

        if config.tls.is_some() != self.tls.with(Option::is_some) {

            return Err(AppError::config("set_tls", "tls cannot be switched on or off on reload; restart instead"));

        }

        let tls = Self::acceptor(&config, self.challenges.clone(), self.tokens.clone(), self.held.clone())?;
        let resolved = Pools::resolve_blocking(&config)?;
        let current = self.swap.load();
        let version = current.snapshot.version + 1;
        let pools = Pools::rebuild(&config, &current.pools, &resolved)?;

        Models::install(&config)?;

        let snapshot = Snapshot::build(config, version)?;

        self.swap.store(Runtime { snapshot: Arc::new(snapshot), pools: Arc::new(pools) });
        self.tls.store(tls);

        if let Ok(mut names) = self.names.lock() { *names = resolved; }

        Ok(version)

    }

    pub fn challenges ( &self ) -> Option<Challenges> {

        self.challenges.clone()

    }

    pub fn tokens ( &self ) -> Option<Tokens> {

        self.tokens.clone()

    }

    pub fn reissue ( &self ) -> AppResult<()> {

        let current = self.swap.load();

        self.tls.store(Self::acceptor(&current.snapshot.config, self.challenges.clone(), self.tokens.clone(), self.held.clone())?);

        Ok(())

    }

    pub fn resolved ( &self, host: &str ) -> Option<Vec<std::net::SocketAddr>> {

        self.names.lock().ok()?.get(host).cloned()

    }

    pub fn refresh ( &self, fresh: Resolved ) -> AppResult<()> {

        let mut resolved = self.names.lock().map_err(|_| AppError::message("resolver state poisoned"))?.clone();

        resolved.extend(fresh);

        let current = self.swap.load();
        let pools = Pools::rebuild(&current.snapshot.config, &current.pools, &resolved)?;

        self.swap.store(Runtime { snapshot: current.snapshot.clone(), pools: Arc::new(pools) });

        if let Ok(mut names) = self.names.lock() { *names = resolved; }

        Ok(())

    }

    pub fn held ( &self ) -> Held {

        self.held.clone()

    }

    fn acceptor ( config: &Config, challenges: Option<Challenges>, tokens: Option<Tokens>, held: Held ) -> AppResult<Option<Acceptor>> {

        config.tls.as_ref().map(|tls| {

            let managed = tls.acme.as_ref().map(Acme::paths);
            let ( cert, key ) = managed.as_ref().map_or(( tls.cert.as_path(), tls.key.as_path() ), |( cert, key )| ( cert.as_path(), key.as_path() ));
            let default = Bundle { names: &[], cert, key, ocsp: tls.ocsp.as_deref() };
            let named = tls.certificates.iter().map(|certificate| Bundle { names: &certificate.names, cert: &certificate.cert, key: &certificate.key, ocsp: certificate.ocsp.as_deref() });
            let demand = Self::demand(config, held)?;
            let obtain = demand.as_ref().zip(tls.acme.as_ref()).zip(tls.on_demand.as_ref()).map(|( ( demand, acme ), plan )| Arc::new(Summon::new(acme.clone(), plan.clone(), demand.clone(), challenges.clone(), tokens, config)) as Arc<dyn Obtain>);
            let policy = Policy { timeout_ms: tls.handshake_timeout_ms, http2: config.server.http2, resumption: Resumption { sessions: tls.session_cache, tickets: tls.tickets }, client: tls.client_ca.as_deref().map(|ca| ClientPolicy { ca, required: tls.client_auth == ClientAuth::Required }), challenges, demand, obtain, modern: tls.min_version == "1.3" };

            Tls::acceptor(default, named, policy)

        }).transpose()

    }

    fn demand ( config: &Config, held: Held ) -> AppResult<Option<Arc<Demand>>> {

        let Some(tls) = config.tls.as_ref().filter(|tls| tls.internal || tls.on_demand.is_some()) else { return Ok(None); };
        let plan = tls.on_demand.clone().unwrap_or_default();

        let names = match ( plan.names.is_empty(), plan.ask.is_some() ) {
            ( false, _ ) => plan.names,
            ( true, true ) => vec!["*".to_string()],
            ( true, false ) => config.routes.iter().filter_map(|route| route.host.clone()).chain(["localhost".to_string()]).collect(),
        };

        let authority = tls.internal.then(|| Authority::open(&tls.ca_dir, tls.leaf_days)).transpose()?;

        Ok(Some(Arc::new(Demand::new(names, plan.capacity, authority, held))))

    }

}
