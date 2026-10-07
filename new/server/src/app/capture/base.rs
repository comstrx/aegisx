use std::sync::Arc;
use std::time::Instant;

use serde_json::Value;

use crate::config::base::consts::{BACKEND_EVENTS_MAX, BACKEND_FIELD_MAX, DAY_MS, JOURNEY_EVENTS_MAX, SPAN_FIELD_MAX};
use crate::core::sync::Shared;
use crate::core::time::Clock;
use super::arch::{BackendEvent, Capture, Event, Journey, Trace};

impl Capture {

    pub fn new ( recent_cap: usize, journeys_cap: usize ) -> Self {

        Self {
            recent       : std::collections::VecDeque::with_capacity(recent_cap.min(1_024)),
            active       : std::collections::HashMap::new(),
            finished     : std::collections::VecDeque::new(),
            recent_cap,
            journeys_cap,
            outbox       : None,
            dropped      : 0,
        }

    }

    pub fn exporting ( self, on: bool ) -> Self {

        Self { outbox: on.then(Vec::new), ..self }

    }

    pub fn drain ( &mut self ) -> Vec<Journey> {

        self.outbox.as_mut().map(std::mem::take).unwrap_or_default()

    }

    pub fn recent ( &self ) -> impl Iterator<Item = &Event> {

        self.recent.iter()

    }

    pub fn active ( &self ) -> impl Iterator<Item = &Journey> {

        self.active.values()

    }

    pub fn active_count ( &self ) -> usize {

        self.active.len()

    }

    pub fn dropped ( &self ) -> u64 {

        self.dropped

    }

    pub fn find ( &self, id: &str ) -> Option<&Journey> {

        self.active.get(id).or_else(|| self.finished.iter().rev().find(|journey| &*journey.request_id == id))

    }

    pub fn annotate ( &mut self, id: &str, stage: &'static str, elapsed_ms: u64, details: Value ) {

        let journey = match self.active.get_mut(id) {
            Some(journey) => Some(journey),
            None => self.finished.iter_mut().rev().find(|journey| &*journey.request_id == id),
        };

        let request_id: Arc<str> = journey.as_ref().map_or_else(|| Arc::from(id), |journey| journey.request_id.clone());
        let sequence = journey.as_ref().and_then(|journey| journey.events.last()).map_or(1, |event| event.sequence + 1);
        let event = Event { request_id, sequence, stage, elapsed_ms, timestamp_ms: Clock::now_ms(), details };

        if let Some(journey) = journey { journey.push(event.clone()); }

        Self::remember(&mut self.recent, self.recent_cap, event);

    }

    pub fn backend ( &mut self, event: BackendEvent ) -> Option<()> {

        let journey = self.active.get_mut(event.request_id.as_str())?;

        if journey.backend_events.len() >= BACKEND_EVENTS_MAX { journey.truncated = true; return None; }

        let mut event = event;
        let elapsed = Clock::now_ms().saturating_sub(journey.started_ms).min(DAY_MS);

        if event.elapsed_ms.is_none() { event.elapsed_ms = Some(elapsed); }

        let details = serde_json::to_value(&event).unwrap_or(Value::Null);
        let record = Event { request_id: journey.request_id.clone(), sequence: 0, stage: "backend_reported", elapsed_ms: elapsed, timestamp_ms: Clock::now_ms(), details };

        Self::remember(&mut self.recent, self.recent_cap, record.clone());
        journey.push(record);
        journey.backend_events.push(event);

        Some(())

    }

    fn remember ( recent: &mut std::collections::VecDeque<Event>, cap: usize, event: Event ) {

        while recent.len() >= cap { recent.pop_front(); }

        recent.push_back(event);

    }

    fn open ( &mut self, journey: Journey ) -> bool {

        if self.active.len() >= self.journeys_cap { self.dropped += 1; return false; }

        self.active.insert(journey.request_id.clone(), journey);

        true

    }

    fn close ( &mut self, id: &str ) {

        let Some(journey) = self.active.remove(id) else { return; };

        if let Some(outbox) = &mut self.outbox && outbox.len() < self.journeys_cap { outbox.push(journey.clone()); }

        while self.finished.len() >= self.journeys_cap { self.finished.pop_front(); }

        self.finished.push_back(journey);

    }

}

impl Journey {

    fn push ( &mut self, event: Event ) {

        if self.events.len() < JOURNEY_EVENTS_MAX { self.events.push(event); } else { self.truncated = true; }

    }

}

impl Trace {

    pub fn begin ( capture: &Shared<Capture>, id: &str, route: &Arc<str>, actor: String, started: Instant, details: Value ) -> Self {

        let id: Arc<str> = Arc::from(id);
        let journey = Journey { request_id: id.clone(), route: route.clone(), actor, started_ms: Clock::now_ms(), events: Vec::with_capacity(4), backend_events: Vec::new(), truncated: false };
        let tracked = capture.with_mut(|capture| capture.open(journey));
        let mut trace = Self { capture: capture.clone(), id, started, sequence: 0, tracked };

        trace.record("received", details);

        trace

    }

    pub fn record ( &mut self, stage: &'static str, details: Value ) {

        self.sequence += 1;

        let event = Event {
            request_id   : self.id.clone(),
            sequence     : self.sequence,
            stage,
            elapsed_ms   : Clock::elapsed_ms(self.started),
            timestamp_ms : Clock::now_ms(),
            details,
        };

        let tracked = self.tracked;

        self.capture.with_mut(|capture| {

            Capture::remember(&mut capture.recent, capture.recent_cap, event.clone());

            if tracked && let Some(journey) = capture.active.get_mut(&event.request_id) { journey.push(event); }

        });

    }

    pub fn finish ( mut self, stage: &'static str, details: Value ) {

        self.record(stage, details);

        if self.tracked { self.capture.with_mut(|capture| capture.close(&self.id)); }

    }

}

impl BackendEvent {

    pub fn valid ( &self ) -> bool {

        let name = |value: &str| !value.is_empty() && value.len() <= BACKEND_FIELD_MAX && value.bytes().all(|byte| byte.is_ascii_alphanumeric() || b"._:/-".contains(&byte));
        let span = |value: &Option<String>| value.as_ref().is_none_or(|value| !value.is_empty() && value.len() <= SPAN_FIELD_MAX && value.bytes().all(|byte| byte.is_ascii_alphanumeric() || b"_-".contains(&byte)));

        crate::app::Identity::looks_like_uuid(self.request_id.as_bytes())
            && name(&self.service)
            && name(&self.operation)
            && matches!(self.state.as_str(), "started" | "completed" | "failed")
            && span(&self.span_id)
            && span(&self.parent_id)
            && (self.span_id.is_none() || self.span_id != self.parent_id)
            && self.elapsed_ms.is_none_or(|value| value <= DAY_MS)
            && self.duration_ms.is_none_or(|value| value <= DAY_MS)

    }

}
