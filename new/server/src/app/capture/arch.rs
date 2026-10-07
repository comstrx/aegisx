use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use std::time::Instant;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::core::sync::Shared;

#[derive(Clone, Debug, Serialize)]
pub struct Event {
    pub request_id   : Arc<str>,
    pub sequence     : u32,
    pub stage        : &'static str,
    pub elapsed_ms   : u64,
    pub timestamp_ms : u64,
    pub details      : Value,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BackendEvent {
    pub request_id  : String,
    pub service     : String,
    pub operation   : String,
    pub state       : String,
    pub duration_ms : Option<u64>,
    #[serde(default)]
    pub span_id     : Option<String>,
    #[serde(default)]
    pub parent_id   : Option<String>,
    #[serde(default)]
    pub elapsed_ms  : Option<u64>,
}

#[derive(Clone, Debug, Serialize)]
pub struct Journey {
    pub request_id     : Arc<str>,
    pub route          : Arc<str>,
    pub actor          : String,
    pub started_ms     : u64,
    pub events         : Vec<Event>,
    pub backend_events : Vec<BackendEvent>,
    pub truncated      : bool,
}

pub struct Capture {
    pub(super) recent       : VecDeque<Event>,
    pub(super) active       : HashMap<Arc<str>, Journey>,
    pub(super) finished     : VecDeque<Journey>,
    pub(super) recent_cap   : usize,
    pub(super) journeys_cap : usize,
    pub(super) dropped      : u64,
    pub(super) outbox       : Option<Vec<Journey>>,
}

pub struct Trace {
    pub(super) capture  : Shared<Capture>,
    pub(super) id       : Arc<str>,
    pub(super) started  : Instant,
    pub(super) sequence : u32,
    pub(super) tracked  : bool,
}
