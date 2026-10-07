use std::time::Instant;

use foldhash::fast::RandomState;
use papaya::HashMap;

pub const FOREVER: u64 = u64::MAX;

pub struct Entry <V> {
    pub(super) value   : V,
    pub(super) expires : u64,
}

pub trait Weigh {
    fn weight ( &self ) -> u64;
}

#[derive(Clone, Copy, Debug, Default)]
pub struct ByWeight;

pub struct Weighted <K: Eq + std::hash::Hash, V: Clone + Weigh> {
    pub(super) inner : quick_cache::sync::Cache<K, V, ByWeight, RandomState>,
    pub(super) bytes : u64,
}

pub struct Cache <K, V> {
    pub(super) map      : HashMap<K, Entry<V>, RandomState>,
    pub(super) capacity : usize,
    pub(super) ttl_ms   : u64,
    pub(super) started  : Instant,
}

pub struct Slot {
    pub(super) size       : u64,
    pub(super) expires_ms : u64,
    pub(super) used       : u64,
}

#[derive(Default)]
pub struct Ledger {
    pub(super) slots : std::collections::HashMap<u128, Slot>,
    pub(super) bytes : u64,
    pub(super) tick  : u64,
}

pub struct Shelf {
    pub(super) root     : std::path::PathBuf,
    pub(super) capacity : u64,
    pub(super) ledger   : std::sync::Mutex<Ledger>,
}
