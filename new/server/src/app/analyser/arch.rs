use std::sync::Arc;
use std::sync::atomic::AtomicU64;
use std::time::Instant;

use crate::app::{BackendEvent, Capture, Telemetry};
use crate::config::AnalysisConfig;
use crate::core::queue::Queue;
use crate::core::sync::Shared;
use crate::http::body::Probe;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Facts {
    pub read          : bool,
    pub write         : bool,
    pub path_length   : usize,
    pub query_length  : usize,
    pub header_count  : usize,
    pub declared_body : u64,
    pub has_body      : bool,
    pub path_depth    : usize,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Completion {
    pub status         : u16,
    pub elapsed_ms     : u64,
    pub request_bytes  : u64,
    pub response_bytes : u64,
    pub attempts       : u32,
    pub failed         : bool,
}

#[derive(Clone, Debug)]
pub struct Envelope {
    pub request_id         : Arc<str>,
    pub route              : Arc<str>,
    pub worker             : usize,
    pub admission          : [f32; 16],
    pub sample             : Vec<u8>,
    pub sample_seen        : usize,
    pub response_sample    : Vec<u8>,
    pub response_seen      : usize,
    pub response_available : bool,
    pub events_truncated   : bool,
    pub outcome            : [f32; 8],
    pub backend_events     : Vec<BackendEvent>,
    pub started            : Instant,
}

#[derive(Default)]
pub struct Report {
    pub submitted : AtomicU64,
    pub dropped   : AtomicU64,
    pub finished  : AtomicU64,
    pub failed    : AtomicU64,
    pub expired   : AtomicU64,
}

pub struct Pending {
    pub analyser  : Arc<Analyser>,
    pub capture   : Shared<Capture>,
    pub worker    : usize,
    pub id        : Arc<str>,
    pub route     : Arc<str>,
    pub admission : [f32; 16],
    pub request   : Probe,
    pub response  : Probe,
    pub started   : Instant,
    pub outcome   : Completion,
}

pub struct Analyser {
    pub(super) config    : AnalysisConfig,
    pub(super) queue     : Queue<Envelope>,
    pub(super) report    : Arc<Report>,
    pub(super) telemetry : Arc<Telemetry>,
}
