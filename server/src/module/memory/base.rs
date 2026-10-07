use std::collections::HashMap;
use std::hash::{Hash, BuildHasher};
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};

use super::arch::{Bucket, Entry, Memory, Shard, Snapshot};

impl<K: Eq + Hash> Memory<K> {

    #[cfg(test)]
    pub fn new ( capacity: usize ) -> Self {

        Self::configured(capacity, 16, 120000)

    }

    pub fn configured ( capacity: usize, shards: usize, idle_ttl_ms: u64 ) -> Self {

        let count = shards.min(capacity).max(1);
        Self {
            shards: (0..count).map(|index| Shard {
                entries: Mutex::new(HashMap::new()), capacity: capacity / count + usize::from(index < capacity % count),
                next_cleanup_ms: AtomicU64::new(0),
            }).collect(), hasher: Default::default(), idle_ttl_ms,
        }

    }

    fn shard ( &self, actor: &K ) -> &Shard<K> {
        &self.shards[self.hasher.hash_one(actor) as usize % self.shards.len()]
    }

    pub fn sources ( &self ) -> usize {
        self.shards.iter().map(|shard| shard.entries.lock().unwrap_or_else(|error| error.into_inner()).len()).sum()
    }

    pub fn begin ( &self, actor: K, now_ms: u64 ) -> Option<Snapshot> {

        let shard = self.shard(&actor);
        let mut entries = shard.entries.lock().unwrap_or_else(|error| error.into_inner());
        if entries.get(&actor).is_some_and(|entry| entry.in_flight == 0 && now_ms.saturating_sub(entry.last_ms) >= self.idle_ttl_ms) {
            entries.remove(&actor);
        }
        if entries.len() >= shard.capacity && !entries.contains_key(&actor) {
            if now_ms >= shard.next_cleanup_ms.load(Ordering::Relaxed) {
                entries.retain(|_, entry| entry.in_flight > 0 || now_ms.saturating_sub(entry.last_ms) < self.idle_ttl_ms);
                shard.next_cleanup_ms.store(now_ms.saturating_add(1000), Ordering::Relaxed);
            }
            if entries.len() >= shard.capacity { return None; }
        }
        let entry = entries.entry(actor).or_insert_with(|| Entry {
            first_ms: now_ms, requests_seen: 0, last_request_ms: now_ms, last_ms: now_ms, in_flight: 0, latency_ms: 0, last_completion_ms: 0,
            buckets: [Bucket::default(); 10], totals: Bucket::default(), latest_second: now_ms / 1000,
        });
        let second = entry.advance(now_ms / 1000);
        let snapshot = Snapshot {
            requests: entry.totals.requests, failures: entry.totals.failures, blocks: entry.totals.blocks,
            in_flight: entry.in_flight,
            gap_seconds: if entry.requests_seen == 0 { 60.0 } else { (now_ms.saturating_sub(entry.last_request_ms)) as f32 / 1000.0 },
            age_seconds: (now_ms.saturating_sub(entry.first_ms)) as f32 / 1000.0,
            latency_ms: entry.latency_ms,
        };
        let bucket = &mut entry.buckets[second as usize % 10];
        if bucket.second != second { *bucket = Bucket { second, ..Bucket::default() }; }
        bucket.requests = bucket.requests.saturating_add(1);
        entry.totals.requests = entry.totals.requests.saturating_add(1);
        entry.in_flight = entry.in_flight.saturating_add(1);
        entry.requests_seen = entry.requests_seen.saturating_add(1);
        entry.last_request_ms = entry.last_request_ms.max(now_ms);
        entry.last_ms = entry.last_ms.max(now_ms);

        Some(snapshot)

    }

    pub fn finish ( &self, actor: K, now_ms: u64, latency_ms: u64, failed: bool, blocked: bool ) {

        let shard = self.shard(&actor);
        let mut entries = shard.entries.lock().unwrap_or_else(|error| error.into_inner());
        if let Some(entry) = entries.get_mut(&actor) {
            entry.in_flight = entry.in_flight.saturating_sub(1);
            if now_ms >= entry.last_completion_ms {
                entry.latency_ms = latency_ms;
                entry.last_completion_ms = now_ms;
            }
            entry.last_ms = entry.last_ms.max(now_ms);
            let second = entry.advance(now_ms / 1000);
            let bucket = &mut entry.buckets[second as usize % 10];
            if bucket.second != second { *bucket = Bucket { second, ..Bucket::default() }; }
            bucket.failures = bucket.failures.saturating_add(u64::from(failed));
            bucket.blocks = bucket.blocks.saturating_add(u64::from(blocked));
            entry.totals.failures = entry.totals.failures.saturating_add(u64::from(failed));
            entry.totals.blocks = entry.totals.blocks.saturating_add(u64::from(blocked));
        }

    }

}

impl Entry {
    fn advance ( &mut self, second: u64 ) -> u64 {
        // Calls can acquire the shard out of timestamp order. Never move its window backwards.
        if second > self.latest_second {
            for bucket in &mut self.buckets {
                if second.saturating_sub(bucket.second) >= 10 {
                    self.totals.requests = self.totals.requests.saturating_sub(bucket.requests);
                    self.totals.failures = self.totals.failures.saturating_sub(bucket.failures);
                    self.totals.blocks = self.totals.blocks.saturating_sub(bucket.blocks);
                    *bucket = Bucket::default();
                }
            }
            self.latest_second = second;
        }
        self.latest_second
    }
}

#[cfg(test)]
mod tests {

    use super::*;

    #[test]
    fn concurrent_context_is_bounded_and_counts_exactly () {
        let memory = std::sync::Arc::new(Memory::<u64>::configured(128, 8, 10000));
        std::thread::scope(|scope| {
            for _ in 0..8 {
                let memory = memory.clone();
                scope.spawn(move || {
                    for _ in 0..100 { memory.begin(7, 1000).unwrap(); memory.finish(7, 1001, 1, false, false); }
                });
            }
        });
        let snapshot = memory.begin(7, 2000).unwrap();
        assert_eq!(snapshot.requests, 800);
        assert_eq!(snapshot.in_flight, 0);
        for actor in 0..10000 { let _ = memory.begin(actor, 2000); }
        assert!(memory.sources() <= 128);
    }

    #[test]
    fn history_expires_and_active_sources_are_not_evicted () {

        let memory: Memory<std::net::IpAddr> = Memory::new(1);
        let first = "127.0.0.1".parse().unwrap();
        let other = "127.0.0.2".parse().unwrap();
        assert_eq!(memory.begin(first, 1000).unwrap().requests, 0);
        assert_eq!(memory.begin(first, 2000).unwrap().requests, 1);
        assert!(memory.begin(other, 130000).is_none());
        memory.finish(first, 3000, 50, true, false);
        memory.finish(first, 3000, 50, false, false);
        let snapshot = memory.begin(first, 4000).unwrap();
        assert_eq!(snapshot.failures, 1);
        assert_eq!(snapshot.in_flight, 0);
        memory.finish(first, 5000, 40, false, false);
        assert_eq!(memory.begin(first, 15000).unwrap().requests, 0);
        memory.finish(first, 15000, 1, false, false);
        assert!(memory.begin(other, 140000).is_some());

    }

    #[test]
    fn rolling_totals_match_timestamped_events_across_window_edges () {
        let memory=Memory::<u64>::configured(1,1,120000);
        let mut requests=Vec::new();let mut failures=Vec::new();let mut blocks=Vec::new();
        for step in 0..4000u64 {
            let now=step*17 + (step/119)*31000;
            let second=now/1000;
            let snapshot=memory.begin(1,now).unwrap();
            let recent=|values: &[u64]| values.iter().filter(|value| second.saturating_sub(**value)<10).count() as u64;
            assert_eq!(snapshot.requests,recent(&requests));
            assert_eq!(snapshot.failures,recent(&failures));
            assert_eq!(snapshot.blocks,recent(&blocks));
            assert_eq!(snapshot.in_flight,0);
            requests.push(second);
            memory.finish(1,now,3,step%3==0,step%7==0);
            if step%3==0 {failures.push(second);}
            if step%7==0 {blocks.push(second);}
        }
    }
    #[test]
    fn delayed_lock_acquisition_cannot_rewind_the_window () {
        let memory=Memory::<u64>::configured(1,1,120000);
        memory.begin(1,20000).unwrap();
        assert_eq!(memory.begin(1,10000).unwrap().requests,1);
        memory.finish(1,9000,1,true,false);
        assert_eq!(memory.begin(1,21000).unwrap().failures,1);
        let snapshot=memory.begin(1,31000).unwrap();
        assert_eq!(snapshot.requests,0);
        assert_eq!(snapshot.failures,0);
        assert_eq!(snapshot.in_flight,2);
        memory.finish(1,32000,25,false,false);
        memory.finish(1,30000,99,false,false);
        assert_eq!(memory.begin(1,33000).unwrap().latency_ms,25);
    }

}
