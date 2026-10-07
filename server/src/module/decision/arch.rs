use std::sync::Arc;
use std::time::Instant;
use crate::core::domain::Actor;
use crate::module::{inference::{Features, Inference, ModelInfo}, memory::Memory};

pub struct Engine {
    pub history: Arc<History>,
    pub verdicts: Option<crate::module::verdict::Verdicts>,
    pub model_info: Option<ModelInfo>,
    pub inference: Option<Inference>,
    pub(super) features: Features,
}

pub struct History {
    pub global: Memory<Actor>,
    pub routes: Memory<(Arc<str>, Actor)>,
    pub started: Instant,
}

pub struct HistoryGuard {
    pub(super) history: Arc<History>,
    pub(super) actor: Actor,
    pub(super) global: bool,
    pub(super) route: Option<Arc<str>>,
    pub(super) started: Instant,
}

#[derive(Default)]
pub struct Inspection {
    pub status: Option<u16>,
    pub reason: String,
    pub features: Option<[f32; 16]>,
    pub score: Option<f32>,
    pub model_state: &'static str,
    pub model_rejected: bool,
    pub cached: bool,
    pub denial_cached: bool,
    pub guard: Option<HistoryGuard>,
}
