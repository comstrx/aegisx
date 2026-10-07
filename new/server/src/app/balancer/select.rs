use std::sync::Arc;
use std::sync::atomic::Ordering;

use crate::config::Balance;
use super::arch::{Backend, Counts, HashKey, Hint, Picker, PoolState, Pools};

const PUBLISH_EVERY: u32 = 32;

impl Backend {

    pub fn available ( &self, now: u64 ) -> bool {

        self.down_until.load(Ordering::Relaxed) <= now && self.reachable()

    }

    pub fn reachable ( &self ) -> bool {

        !self.down
            && self.probed.load(Ordering::Relaxed)
            && (self.limit == 0 || self.active.load(Ordering::Relaxed) < self.limit)

    }

    pub fn succeed ( &self ) {

        if self.fails.load(Ordering::Relaxed) != 0 { self.fails.store(0, Ordering::Relaxed); }

    }

    pub fn fail ( &self, pool: &PoolState, now: u64 ) -> bool {

        let fails = self.fails.fetch_add(1, Ordering::Relaxed) + 1;

        if fails < pool.max_fails { return false; }

        self.fails.store(0, Ordering::Relaxed);

        let ejected = pool.backends.iter().filter(|other| other.index != self.index && other.down_until.load(Ordering::Relaxed) > now).count();

        if (ejected + 1) * 100 > pool.max_ejected as usize * pool.backends.len() { return false; }

        self.revived.store(now + pool.cooldown_ms, Ordering::Relaxed);

        self.down_until.fetch_max(now + pool.cooldown_ms, Ordering::Relaxed) <= now

    }

    pub fn share ( &self, pool: &PoolState, now: u64 ) -> u64 {

        let full = u64::from(self.weight.max(1)) * 64;
        let revived = self.revived.load(Ordering::Relaxed);
        let since = now.saturating_sub(revived);

        match revived != 0 && since < pool.slow_start { true => (full * since.max(1) / pool.slow_start).max(1), false => full }

    }

    pub fn count ( &self, worker: usize ) -> Option<&Counts> {

        self.counts.get(worker)

    }

    pub fn totals ( &self ) -> [u64; 3] {

        self.counts.iter().fold([0; 3], |[served, failed, retried], counts| [served + counts.served.get(), failed + counts.failed.get(), retried + counts.retried.get()])

    }

}

impl Picker {

    pub fn pick ( &mut self, pools: &Pools, pool: &PoolState, excluded: &[usize], now: u64, hint: Hint<'_> ) -> Option<usize> {

        self.fit(pools);

        let open = |backend: &Arc<Backend>| !excluded.contains(&backend.index) && backend.available(now);

        if let Some(sticky) = &pool.sticky && let Some(index) = sticky.wanted(hint.headers, &pool.backends) && pool.backends.get(index).is_some_and(&open) { return Some(index); }

        let primary = |backend: &Arc<Backend>| !backend.backup && open(backend);

        let chosen = match self.choose(pool, &primary, hint, now) {
            None if pool.spare => self.choose(pool, &|backend: &Arc<Backend>| backend.backup && open(backend), hint, now),
            chosen => chosen,
        };

        chosen.or_else(|| self.choose(pool, &|backend: &Arc<Backend>| !excluded.contains(&backend.index) && backend.reachable(), hint, now))

    }

    fn choose ( &mut self, pool: &PoolState, open: &dyn Fn(&Arc<Backend>) -> bool, hint: Hint<'_>, now: u64 ) -> Option<usize> {

        match ( pool.policy, &pool.hash ) {
            ( Balance::First, _ ) => pool.backends.iter().position(open),
            ( Balance::RoundRobin, _ ) => self.weighted(pool, open, now),
            ( Balance::LeastConn, _ ) => self.least(pool, open, now),
            ( Balance::Random, _ ) => self.two(pool, open, now),
            ( Balance::LeastTime, _ ) => self.fastest(pool, open, now),
            ( Balance::IpHash | Balance::Hash, Some(key) ) => HashKey::rendezvous(&pool.backends, key.digest(hint), open),
            ( Balance::IpHash | Balance::Hash, None ) => self.weighted(pool, open, now),
        }

    }

    pub fn observe ( &mut self, backend: &Backend, pool: usize, elapsed_us: u64 ) {

        let ( Some(latency), Some(samples) ) = ( self.latency.get_mut(pool).and_then(|slots| slots.get_mut(backend.index)), self.samples.get_mut(pool).and_then(|slots| slots.get_mut(backend.index)) ) else { return; };

        *latency = if *samples == 0 { elapsed_us } else { (*latency * 7 + elapsed_us) / 8 };
        *samples = samples.wrapping_add(1);

        if samples.is_multiple_of(PUBLISH_EVERY) { backend.latency_us.store(*latency, Ordering::Relaxed); }

    }

    fn weighted ( &mut self, pool: &PoolState, open: &dyn Fn(&Arc<Backend>) -> bool, now: u64 ) -> Option<usize> {

        if pool.backends.len() == 1 { return pool.backends.first().filter(|backend| open(backend)).map(|_| 0); }

        let current = self.current.get_mut(pool.index)?;
        let mut total = 0i64;
        let mut best: Option<usize> = None;

        for ( index, backend ) in pool.backends.iter().enumerate() {

            if !open(backend) { continue; }

            let weight = backend.share(pool, now) as i64;

            current[index] += weight;
            total += weight;

            if best.is_none_or(|chosen| current[index] > current[chosen]) { best = Some(index); }

        }

        let chosen = best?;

        current[chosen] -= total;

        Some(chosen)

    }

    fn fastest ( &mut self, pool: &PoolState, open: &dyn Fn(&Arc<Backend>) -> bool, now: u64 ) -> Option<usize> {

        let latency = self.latency.get(pool.index)?;

        pool.backends.iter().enumerate().filter(|( _, backend )| open(backend)).min_by_key(|( index, backend )| {

            u128::from(latency.get(*index).copied().unwrap_or(0).max(1)) * u128::from(backend.active.load(Ordering::Relaxed) + 1) * 64 / u128::from(backend.share(pool, now))

        }).map(|( index, _ )| index)

    }

    fn two ( &mut self, pool: &PoolState, open: &dyn Fn(&Arc<Backend>) -> bool, now: u64 ) -> Option<usize> {

        let count = pool.backends.len();
        let first = self.roll(count);
        let second = (first + 1 + self.roll(count.saturating_sub(1))) % count.max(1);
        let load = |index: usize| pool.backends.get(index).filter(|backend| open(backend)).map(|backend| (backend.active.load(Ordering::Relaxed) + 1) * 64_000 / backend.share(pool, now));

        match ( load(first), load(second) ) {
            ( Some(one), Some(other) ) => Some(if other < one { second } else { first }),
            ( Some(_), None ) => Some(first),
            ( None, Some(_) ) => Some(second),
            ( None, None ) => pool.backends.iter().position(open),
        }

    }

    fn roll ( &mut self, bound: usize ) -> usize {

        self.seed ^= self.seed << 13;
        self.seed ^= self.seed >> 7;
        self.seed ^= self.seed << 17;

        (self.seed % bound.max(1) as u64) as usize

    }

    fn least ( &mut self, pool: &PoolState, open: &dyn Fn(&Arc<Backend>) -> bool, now: u64 ) -> Option<usize> {

        let cursor = self.cursors.get_mut(pool.index)?;
        let count = pool.backends.len();
        let mut best: Option<( usize, u128 )> = None;

        for offset in 0..count {

            let index = (*cursor + offset) % count;
            let backend = &pool.backends[index];

            if !open(backend) { continue; }

            let load = (u128::from(backend.active.load(Ordering::Relaxed)) + 1) * 64_000_000 / u128::from(backend.share(pool, now));

            if best.is_none_or(|( _, current )| load < current) { best = Some(( index, load )); }

        }

        let ( chosen, _ ) = best?;

        *cursor = (chosen + 1) % count;

        Some(chosen)

    }

}
