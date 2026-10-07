use std::thread::JoinHandle;
use serde::Deserialize;
use ort::session::Session;
use tokio::sync::mpsc;
use std::{sync::{Arc, atomic::{AtomicBool,AtomicU64}}, time::Instant};
use super::Envelope;



pub struct Model {
    pub(super) session: Session,
    pub info: ModelInfo,
}

pub(super) enum Message {
    Background(Box<Envelope>, Instant, Box<dyn FnOnce(Analysis) + Send>),
    Stop,
}
pub struct Analysis { pub inputs: serde_json::Value, pub scores: Option<crate::core::domain::RiskScores>, pub features: super::Vector, pub score: Option<f32>, pub queue_ms: u64, pub inference_us: u64, pub journal_us: u64, pub expired: bool }
#[derive(Default)]
pub struct Counters { pub processing: AtomicU64, pub journal_unhealthy: AtomicBool, pub journal_us: AtomicU64, pub recovered: AtomicU64, pub journal_failures: AtomicU64, pub stopping: AtomicBool, pub submitted: AtomicU64, pub finished: AtomicU64, pub dropped: AtomicU64, pub rejected: AtomicU64, pub expired: AtomicU64, pub failed: AtomicU64, pub total_us: AtomicU64, pub last_us: AtomicU64 }
pub struct Reservation { pub(super) permit: mpsc::OwnedPermit<Message> }
#[derive(Clone)]
pub struct Inference { pub(super) journal_enabled: bool, pub(super) sender: mpsc::Sender<Message>, pub(super) counters: Arc<Counters> }
pub struct InferenceGuard { pub(super) sender: mpsc::Sender<Message>, pub(super) counters: Arc<Counters>, pub(super) thread: Option<JoinHandle<()>> }

#[derive(Clone, serde::Serialize, Deserialize)]
pub struct ModelInfo {
    #[serde(default = "default_precision")]
    pub precision: String,
    #[serde(default)]
    pub input_schema: Option<String>,
    pub model_version: String,
    pub artifact_sha256: String,
    pub feature_version: u32,
    pub parameter_count: usize,
    pub source: String,
    #[serde(default)]
    pub deployment_ready: bool,
    #[serde(default)]
    pub evaluation_notice: Option<String>,
}

fn default_precision () -> String {"fp32".into()}
