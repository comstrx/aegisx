use std::sync::atomic::{AtomicBool, AtomicU64};
use std::sync::mpsc::SyncSender;
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Instant;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Spec {
    pub name        : &'static str,
    pub workers     : usize,
    pub capacity    : usize,
    pub deadline_ms : u64,
}

#[derive(Default)]
pub struct Stats {
    pub submitted : AtomicU64,
    pub rejected  : AtomicU64,
    pub expired   : AtomicU64,
    pub finished  : AtomicU64,
    pub failed    : AtomicU64,
    pub active    : AtomicU64,
    pub wait_us   : AtomicU64,
    pub total_us  : AtomicU64,
    pub last_us   : AtomicU64,
}

pub struct Token {
    pub(super) stopping : Arc<AtomicBool>,
    pub(super) deadline : Instant,
}

pub struct Job <J> {
    pub(super) payload   : J,
    pub(super) submitted : Instant,
    pub(super) deadline  : Instant,
}

pub struct Queue <J> {
    pub(super) spec     : Spec,
    pub(super) sender   : Mutex<Option<SyncSender<Job<J>>>>,
    pub(super) stats    : Arc<Stats>,
    pub(super) stopping : Arc<AtomicBool>,
    pub(super) workers  : Mutex<Vec<JoinHandle<()>>>,
}
