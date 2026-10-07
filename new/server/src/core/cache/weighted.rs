use std::hash::Hash;

use foldhash::fast::RandomState;
use quick_cache::sync::{Cache, DefaultLifecycle};
use quick_cache::Weighter;

use super::arch::{ByWeight, Weigh, Weighted};

impl <K, V: Weigh> Weighter<K, V> for ByWeight {

    fn weight ( &self, _: &K, value: &V ) -> u64 {

        value.weight().max(1)

    }

}

impl <K: Eq + Hash, V: Clone + Weigh> Weighted <K, V> {

    pub fn new ( items: usize, bytes: u64 ) -> Self {

        Self { inner: Cache::with(items.max(16), bytes.max(1), ByWeight, RandomState::default(), DefaultLifecycle::default()), bytes }

    }

    pub fn get ( &self, key: &K ) -> Option<V> {

        self.inner.get(key)

    }

    pub fn insert ( &self, key: K, value: V ) -> bool {

        if value.weight() > self.bytes { return false; }

        self.inner.insert(key, value);

        true

    }

    pub fn remove ( &self, key: &K ) -> bool {

        self.inner.remove(key).is_some()

    }

    pub fn clear ( &self ) {

        self.inner.clear();

    }

    pub fn len ( &self ) -> usize {

        self.inner.len()

    }

    pub fn is_empty ( &self ) -> bool {

        self.inner.is_empty()

    }

    pub fn weight ( &self ) -> u64 {

        self.inner.weight()

    }

    pub fn capacity ( &self ) -> u64 {

        self.bytes

    }

}
