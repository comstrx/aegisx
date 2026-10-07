use std::sync::Arc;
use std::sync::atomic::{AtomicU32, AtomicU64};

use aws_lc_rs::hmac;
use foldhash::fast::RandomState;
use papaya::HashMap;

pub type Key = [u8; 32];

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct History {
    pub requests    : u32,
    pub failures    : u32,
    pub blocks      : u32,
    pub in_flight   : u32,
    pub gap_seconds : f32,
    pub age_seconds : f32,
    pub latency_ms  : u32,
}

pub struct Actor {
    pub(super) first_ms   : u64,
    pub(super) last_ms    : AtomicU64,
    pub(super) window     : AtomicU64,
    pub(super) requests   : AtomicU32,
    pub(super) failures   : AtomicU32,
    pub(super) blocks     : AtomicU32,
    pub(super) in_flight  : AtomicU32,
    pub(super) latency_ms : AtomicU32,
    pub(super) due_us     : AtomicU64,
}

pub struct Window {
    pub(super) slot    : AtomicU64,
    pub(super) count   : AtomicU32,
    pub(super) last_ms : AtomicU64,
    pub(super) due_us  : AtomicU64,
}

pub struct Memory {
    pub(super) secret   : hmac::Key,
    pub(super) actors   : HashMap<Key, Arc<Actor>, RandomState>,
    pub(super) routes   : HashMap<( Key, u32 ), Arc<Window>, RandomState>,
    pub(super) capacity : usize,
    pub(super) idle_ms  : u64,
    pub(super) overflow : AtomicU64,
}
