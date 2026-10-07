use std::hash::Hash;
use std::time::Instant;

use foldhash::fast::RandomState;
use papaya::HashMap;

use super::arch::{Cache, Entry, FOREVER};

impl <K, V> Cache <K, V>
where K: Hash + Eq + Clone + Send + Sync + 'static, V: Clone + Send + Sync + 'static {

    pub fn new ( capacity: usize, ttl_ms: u64 ) -> Self {

        let map = HashMap::builder().capacity(capacity.min(1 << 20)).hasher(RandomState::default()).build();

        Self { map, capacity, ttl_ms, started: Instant::now() }

    }

    pub fn get ( &self, key: &K ) -> Option<V> {

        let now = self.now();
        let map = self.map.pin();
        let entry = map.get(key)?;

        if entry.expires <= now { return None; }

        Some(entry.value.clone())

    }

    pub fn put ( &self, key: K, value: V ) -> bool {

        self.put_for(key, value, self.ttl_ms)

    }

    pub fn put_for ( &self, key: K, value: V, ttl_ms: u64 ) -> bool {

        let expires = if ttl_ms == FOREVER { FOREVER } else { self.now().saturating_add(ttl_ms) };
        let map = self.map.pin();

        if self.map.len() >= self.capacity && !map.contains_key(&key) { return false; }

        map.insert(key, Entry { value, expires });

        true

    }

    pub fn remove ( &self, key: &K ) -> Option<V> {

        self.map.pin().remove(key).map(|entry| entry.value.clone())

    }

    pub fn clear ( &self ) {

        self.map.pin().clear();

    }

    pub fn sweep ( &self ) -> usize {

        let now = self.now();
        let before = self.map.len();

        self.map.pin().retain(|_, entry| entry.expires > now);

        before.saturating_sub(self.map.len())

    }

    pub fn len ( &self ) -> usize {

        self.map.len()

    }

    pub fn is_empty ( &self ) -> bool {

        self.map.len() == 0

    }

    pub fn capacity ( &self ) -> usize {

        self.capacity

    }

    pub fn ttl_ms ( &self ) -> u64 {

        self.ttl_ms

    }

    fn now ( &self ) -> u64 {

        self.started.elapsed().as_millis() as u64

    }

}
