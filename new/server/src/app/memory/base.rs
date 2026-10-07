use std::fmt::Write;
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};

use aws_lc_rs::hmac;
use foldhash::fast::RandomState;
use papaya::HashMap;

use crate::core::error::{AppFail, AppResult};
use super::arch::{Actor, History, Key, Memory, Window};

const WINDOW_MS: u64 = 10_000;

impl Memory {

    pub fn new ( capacity: usize, idle_ms: u64, key: Option<[u8; 32]> ) -> AppResult<Self> {

        let mut seed = [0u8; 32];

        match key {
            Some(key) => seed = key,
            None => getrandom::fill(&mut seed).or_fail("cannot read system entropy")?,
        }

        let actors = HashMap::builder().capacity(capacity.min(1 << 16)).hasher(RandomState::default()).build();
        let routes = HashMap::builder().capacity(capacity.min(1 << 16)).hasher(RandomState::default()).build();

        Ok(Self { secret: hmac::Key::new(hmac::HMAC_SHA256, &seed), actors, routes, capacity, idle_ms, overflow: AtomicU64::new(0) })

    }

    pub fn handle ( &self, scope: &[u8], material: &[u8] ) -> Key {

        let mut context = hmac::Context::with_key(&self.secret);

        context.update(scope);
        context.update(b":");
        context.update(material);

        let tag = context.sign();
        let mut key = [0u8; 32];

        key.copy_from_slice(tag.as_ref());

        key

    }

    pub fn hex ( key: &Key ) -> String {

        key.iter().fold(String::with_capacity(64), |mut text, byte| { let _ = write!(text, "{byte:02x}"); text })

    }

    pub fn observe ( &self, key: Key, now_ms: u64 ) -> ( Arc<Actor>, History ) {

        let map = self.actors.pin();

        let actor = match map.get(&key) {
            Some(actor) => actor.clone(),
            None if self.actors.len() >= self.capacity => { self.overflow.fetch_add(1, Ordering::Relaxed); Arc::new(Actor::new(now_ms)) }
            None => map.get_or_insert(key, Arc::new(Actor::new(now_ms))).clone(),
        };

        drop(map);

        let history = actor.observe(now_ms);

        ( actor, history )

    }

    fn window ( &self, key: Key, route: u32, now_ms: u64 ) -> Option<Arc<Window>> {

        let map = self.routes.pin();

        match map.get(&( key, route )) {
            Some(window) => Some(window.clone()),
            None if self.routes.len() >= self.capacity => { self.overflow.fetch_add(1, Ordering::Relaxed); None }
            None => Some(map.get_or_insert(( key, route ), Arc::new(Window { slot: AtomicU64::new(now_ms / WINDOW_MS), count: AtomicU32::new(0), last_ms: AtomicU64::new(now_ms), due_us: AtomicU64::new(0) })).clone()),
        }

    }

    pub fn pace ( &self, key: Key, route: u32, now_us: u64, rate: u32, burst: u32 ) -> Option<u64> {

        let window = self.window(key, route, now_us / 1_000)?;

        window.last_ms.store(now_us / 1_000, Ordering::Relaxed);

        Self::meter(&window.due_us, now_us, rate, burst)

    }

    pub fn pace_all ( &self, rules: impl Iterator<Item = ( Key, u32, u32, u32 )>, now_us: u64 ) -> Option<u64> {

        let held: Vec<( Arc<Window>, u32, u32 )> = rules.filter_map(|( key, scope, rate, burst )| self.window(key, scope, now_us / 1_000).map(|window| ( window, rate, burst ))).collect();

        if let Some(wait) = held.iter().filter_map(|( window, rate, burst )| Self::step(window.due_us.load(Ordering::Relaxed), now_us, *rate, *burst).err()).max() { return Some(wait); }

        for ( window, rate, burst ) in &held {

            window.last_ms.store(now_us / 1_000, Ordering::Relaxed);

            let _ = Self::meter(&window.due_us, now_us, *rate, *burst);

        }

        None

    }

    fn step ( due: u64, now_us: u64, rate: u32, burst: u32 ) -> Result<u64, u64> {

        let interval = 1_000_000 / u64::from(rate.max(1));
        let allowance = interval.saturating_mul(u64::from(burst) + 1);
        let next = due.max(now_us).saturating_add(interval);

        if next - now_us > allowance { Err(next - now_us - allowance) } else { Ok(next) }

    }

    pub fn meter ( due_us: &AtomicU64, now_us: u64, rate: u32, burst: u32 ) -> Option<u64> {

        let mut due = due_us.load(Ordering::Relaxed);

        loop {

            let next = match Self::step(due, now_us, rate, burst) { Ok(next) => next, Err(wait) => return Some(wait) };

            match due_us.compare_exchange_weak(due, next, Ordering::Relaxed, Ordering::Relaxed) {
                Ok(_) => return None,
                Err(actual) => due = actual,
            }

        }

    }

    pub fn charge ( &self, key: Key, route: u32, now_ms: u64 ) -> u32 {

        let Some(window) = self.window(key, route, now_ms) else { return 0; };
        let slot = now_ms / WINDOW_MS;
        let current = window.slot.load(Ordering::Relaxed);

        if current != slot && window.slot.compare_exchange(current, slot, Ordering::Relaxed, Ordering::Relaxed).is_ok() { window.count.store(0, Ordering::Relaxed); }

        window.last_ms.store(now_ms, Ordering::Relaxed);
        window.count.fetch_add(1, Ordering::Relaxed)

    }

    pub fn sweep ( &self, now_ms: u64 ) -> usize {

        let before = self.actors.len() + self.routes.len();
        let idle_ms = self.idle_ms;

        self.actors.pin().retain(|_, actor| actor.in_flight.load(Ordering::Relaxed) > 0 || actor.last_ms.load(Ordering::Relaxed).saturating_add(idle_ms) > now_ms);
        self.routes.pin().retain(|_, window| window.last_ms.load(Ordering::Relaxed).saturating_add(idle_ms) > now_ms);

        before.saturating_sub(self.actors.len() + self.routes.len())

    }

    pub fn len ( &self ) -> usize {

        self.actors.len()

    }

    pub fn is_empty ( &self ) -> bool {

        self.actors.len() == 0

    }

    pub fn overflow ( &self ) -> u64 {

        self.overflow.load(Ordering::Relaxed)

    }

}

impl Actor {

    fn new ( now_ms: u64 ) -> Self {

        Self {
            first_ms   : now_ms,
            last_ms    : AtomicU64::new(now_ms),
            window     : AtomicU64::new(now_ms / WINDOW_MS),
            requests   : AtomicU32::new(0),
            failures   : AtomicU32::new(0),
            blocks     : AtomicU32::new(0),
            in_flight  : AtomicU32::new(0),
            latency_ms : AtomicU32::new(0),
            due_us     : AtomicU64::new(0),
        }

    }

    pub fn pace ( &self, now_us: u64, rate: u32, burst: u32 ) -> Option<u64> {

        Memory::meter(&self.due_us, now_us, rate, burst)

    }

    fn observe ( &self, now_ms: u64 ) -> History {

        let slot = now_ms / WINDOW_MS;
        let window = self.window.load(Ordering::Relaxed);

        if window != slot && self.window.compare_exchange(window, slot, Ordering::Relaxed, Ordering::Relaxed).is_ok() {

            self.requests.store(0, Ordering::Relaxed);
            self.failures.store(0, Ordering::Relaxed);
            self.blocks.store(0, Ordering::Relaxed);

        }

        let last = self.last_ms.swap(now_ms, Ordering::Relaxed);

        History {
            requests    : self.requests.fetch_add(1, Ordering::Relaxed),
            failures    : self.failures.load(Ordering::Relaxed),
            blocks      : self.blocks.load(Ordering::Relaxed),
            in_flight   : self.in_flight.load(Ordering::Relaxed),
            gap_seconds : now_ms.saturating_sub(last) as f32 / 1_000.0,
            age_seconds : now_ms.saturating_sub(self.first_ms) as f32 / 1_000.0,
            latency_ms  : self.latency_ms.load(Ordering::Relaxed),
        }

    }

    pub fn hold ( &self ) {

        self.in_flight.fetch_add(1, Ordering::Relaxed);

    }

    pub fn release ( &self ) {

        self.in_flight.fetch_sub(1, Ordering::Relaxed);

    }

    pub fn finish ( &self, status: u16, elapsed_ms: u64, blocked: bool ) {

        if blocked { self.blocks.fetch_add(1, Ordering::Relaxed); }
        else if status >= 500 { self.failures.fetch_add(1, Ordering::Relaxed); }

        self.latency_ms.store(elapsed_ms.min(u64::from(u32::MAX)) as u32, Ordering::Relaxed);

    }

    pub fn in_flight ( &self ) -> u32 {

        self.in_flight.load(Ordering::Relaxed)

    }

}
