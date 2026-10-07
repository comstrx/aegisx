use std::sync::Arc;

use crate::app::Capture;
use crate::config::TelemetryConfig;
use crate::core::sync::{Shared, Tally};

pub const BUCKETS: [u64; 7] = [1, 5, 10, 25, 50, 100, 500];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    Completed,
    Blocked,
    Failed,
}

#[repr(align(128))]
#[derive(Default)]
pub struct Stats {
    pub inflight       : Tally,
    pub total          : Tally,
    pub active         : Tally,
    pub completed      : Tally,
    pub blocked        : Tally,
    pub failed         : Tally,
    pub request_bytes  : Tally,
    pub response_bytes : Tally,
    pub latency        : [Tally; 8],
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Summary {
    pub total          : u64,
    pub active         : u64,
    pub completed      : u64,
    pub blocked        : u64,
    pub failed         : u64,
    pub request_bytes  : u64,
    pub response_bytes : u64,
    pub latency        : [u64; 8],
}

pub struct Telemetry {
    pub config   : TelemetryConfig,
    pub workers  : Vec<Arc<Stats>>,
    pub captures : Vec<Shared<Capture>>,
}
