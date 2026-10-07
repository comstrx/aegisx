use std::{collections::BTreeMap, sync::{Arc, Mutex}};
use serde::{Deserialize, Serialize};
use crate::core::time::now_ms;
use crate::module::storage::Event;

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BackendEvent {
    pub request_id: String,
    pub service: String,
    pub operation: String,
    pub state: String,
    pub duration_ms: Option<u64>,
    #[serde(default)] pub span_id: Option<String>,
    #[serde(default)] pub parent_id: Option<String>,
    #[serde(default)] pub elapsed_ms: Option<u64>,
}
impl BackendEvent {
    pub fn valid ( &self ) -> bool {
        uuid::Uuid::parse_str(&self.request_id).is_ok()
            && [&self.service,&self.operation].iter().all(|value| !value.is_empty() && value.len() <= 96
                && value.bytes().all(|c| c.is_ascii_alphanumeric() || b"._:/-".contains(&c)))
            && matches!(self.state.as_str(),"started"|"completed"|"failed")
            && [&self.span_id,&self.parent_id].iter().all(|value|value.as_ref().is_none_or(|value|
                !value.is_empty() && value.len()<=64 && value.bytes().all(|byte|byte.is_ascii_alphanumeric() || b"_-".contains(&byte))))
            && (self.span_id.is_none() || self.span_id!=self.parent_id)
            && self.elapsed_ms.is_none_or(|value|value<=86400000)
            && self.duration_ms.is_none_or(|value| value <= 86400000)
    }
}
#[derive(Serialize)]
pub struct Journey {
    pub request_id: String,
    pub route: String,
    pub actor: String,
    pub started_ms: u64,
    #[serde(skip)] pub capture: bool,
    pub events: Vec<Event>,
    pub backend_events: Vec<BackendEvent>,
    pub truncated: bool,
}
pub type Handle = Arc<Mutex<Journey>>;
pub struct Journeys { active: Mutex<BTreeMap<String,Handle>>, capacity: usize }
impl Journeys {
    pub fn new ( capacity: usize ) -> Self { Self { active: Mutex::new(BTreeMap::new()), capacity } }
    pub fn begin ( &self, id: &mut String, route: String, actor: String, capture: bool ) -> Option<Handle> {
        let mut active = self.active.lock().unwrap_or_else(|error| error.into_inner());
        if active.len() >= self.capacity { return None; }
        if active.contains_key(id) { *id=uuid::Uuid::new_v4().to_string(); }
        let handle = Arc::new(Mutex::new(Journey { request_id: id.clone(), route, actor, capture, started_ms: now_ms(),
            events: Vec::with_capacity(12), backend_events: Vec::new(), truncated: false }));
        active.insert(id.clone(), handle.clone()); Some(handle)
    }
    pub fn backend ( &self, mut event: BackendEvent ) -> Option<bool> {
        let active = self.active.lock().unwrap_or_else(|error| error.into_inner());
        let handle = active.get(&event.request_id)?;
        let mut journey = handle.lock().unwrap_or_else(|error| error.into_inner());
        if journey.backend_events.len() >= 32 { journey.truncated = true; return None; }
        if event.elapsed_ms.is_none() {event.elapsed_ms=Some(now_ms().saturating_sub(journey.started_ms).min(86400000));}
        if journey.capture {
            let elapsed=now_ms().saturating_sub(journey.started_ms);
            journey.push(Event { request_id:event.request_id.clone(), sequence:0, stage:"backend_reported".into(),
                timestamp_ms:now_ms(),elapsed_ms:elapsed,details:serde_json::json!(&event) });
        }
        journey.backend_events.push(event); Some(journey.capture)
    }
    pub fn finish ( &self, id: &str ) { self.active.lock().unwrap_or_else(|error| error.into_inner()).remove(id); }
    pub fn snapshot ( &self ) -> serde_json::Value {
        let active = self.active.lock().unwrap_or_else(|error| error.into_inner());
        serde_json::json!({"total":active.len(),"items":active.values().take(100).map(|handle| {
            let journey=handle.lock().unwrap_or_else(|error| error.into_inner());
            serde_json::to_value(&*journey).unwrap_or_default()
        }).collect::<Vec<_>>()})
    }
}
impl Journey {
    pub fn push ( &mut self, event: Event ) {
        if self.events.len() < 64 { self.events.push(event); } else { self.truncated=true; }
    }
}
