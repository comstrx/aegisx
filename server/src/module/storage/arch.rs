use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64};
use std::thread::JoinHandle;
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;
use crate::core::error::AppResult;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Event {
    pub request_id: String,
    pub sequence: u32,
    pub stage: String,
    pub timestamp_ms: u64,
    pub elapsed_ms: u64,
    pub details: serde_json::Value,
}

pub(super) enum Message { Batch(Vec<Event>), Stop }
#[derive(Default)]
pub(super) struct Counters {
    pub dropped: AtomicU64, pub rejected: AtomicU64, pub committed: AtomicU64,
    pub batches: AtomicU64, pub retries: AtomicU64, pub unhealthy: AtomicBool, pub stopping: AtomicBool,
}
pub struct Reservation { pub(super) permit: mpsc::OwnedPermit<Message> }
impl Reservation {
    pub fn commit ( self, events: Vec<Event> ) { self.permit.send(Message::Batch(events)); }
}
#[derive(Clone)]
pub struct Store {
    pub(super) sender: Option<mpsc::Sender<Message>>,
    pub(super) counters: Arc<Counters>,
}
pub struct StoreGuard {
    pub(super) sender: mpsc::Sender<Message>,
    pub(super) counters: Arc<Counters>,
    pub(super) thread: Option<JoinHandle<AppResult<()>>>,
}
