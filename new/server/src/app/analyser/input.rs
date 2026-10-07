use http::header::{HeaderMap, TRANSFER_ENCODING};
use http::{Method, Uri};
use serde_json::{Value, json};

use crate::app::{ADMISSION, Architecture, CONTENT_FIXED, History, Input, OUTCOME, Schema};
use crate::config::base::consts::DAY_MS;
use crate::http::body::{Probe, Tap};
use super::arch::{Envelope, Facts, Completion, Pending};
use super::content::Content;

impl Facts {

    pub fn of ( method: &Method, uri: &Uri, headers: &HeaderMap, declared: u64 ) -> Self {

        let path = uri.path();

        Self {
            read          : matches!(*method, Method::GET | Method::HEAD | Method::OPTIONS),
            write         : matches!(*method, Method::POST | Method::PUT | Method::PATCH | Method::DELETE),
            path_length   : path.len(),
            query_length  : uri.query().map_or(0, str::len),
            header_count  : headers.len(),
            declared_body : declared,
            has_body      : declared > 0 || headers.contains_key(TRANSFER_ENCODING),
            path_depth    : path.split('/').filter(|part| !part.is_empty()).count(),
        }

    }

    pub fn sample ( method: &Method, uri: &Uri, cap: usize ) -> Probe {

        let probe = Tap::new(cap);

        {

            let mut tap = probe.borrow_mut();

            tap.feed(method.as_str().as_bytes());
            tap.feed(b" ");
            tap.feed(uri.path_and_query().map_or("/", |value| value.as_str()).as_bytes());
            tap.feed(b"\n");

        }

        probe

    }

    pub fn admission ( &self, history: History ) -> [f32; ADMISSION] {

        [
            f32::from(self.read),
            f32::from(self.write),
            self.path_length as f32,
            self.query_length as f32,
            self.header_count as f32,
            self.declared_body as f32,
            f32::from(self.has_body),
            history.requests as f32,
            history.failures as f32,
            history.blocks as f32,
            history.in_flight as f32,
            history.gap_seconds,
            history.age_seconds,
            history.latency_ms as f32,
            self.path_depth as f32,
            f32::from(self.query_length > 0),
        ]

    }

}

impl Completion {

    pub fn vector ( &self ) -> [f32; OUTCOME] {

        [self.status as f32, self.elapsed_ms as f32, self.request_bytes as f32, self.response_bytes as f32, self.attempts as f32, f32::from(self.failed), 1.0, 1.0]

    }

}

impl Envelope {

    pub fn raw ( &self, schema: &Schema ) -> Vec<f32> {

        let content = Content::extract(&self.sample, self.sample_seen, schema);
        let mut raw = Vec::with_capacity(schema.count());

        raw.extend_from_slice(&self.admission);
        raw.extend_from_slice(&content[..CONTENT_FIXED]);
        raw.extend_from_slice(&self.outcome);
        raw.extend_from_slice(&content[CONTENT_FIXED..]);

        raw

    }

    pub fn input ( &self, features: Vec<f32>, arch: &Architecture ) -> Input {

        let mut input = Input::empty(arch);
        let text_bytes = arch.text_bytes;

        input.features = features;

        Input::write_text(&mut input.text[..text_bytes], &self.sample);
        Input::write_text(&mut input.text[text_bytes..], &self.response_sample);

        for ( index, event ) in self.backend_events.iter().take(arch.event_count).enumerate() {

            let name = format!("{}\n{}", event.service, event.operation);

            Input::write_text(&mut input.event_text[index * arch.event_bytes..(index + 1) * arch.event_bytes], name.as_bytes());

            let parent = event.parent_id.as_ref().and_then(|parent| self.backend_events[..index].iter().position(|prior| prior.span_id.as_ref() == Some(parent)));

            let values = [
                1.0,
                f32::from(event.state == "started"),
                f32::from(event.state == "completed"),
                f32::from(event.state == "failed"),
                Self::time(event.duration_ms.unwrap_or(0)),
                Self::time(event.elapsed_ms.unwrap_or(0)),
                parent.map_or(0.0, |parent| (parent + 1) as f32 / arch.event_count as f32),
                f32::from(parent.is_some()),
            ];

            let slot = &mut input.event_values[index * arch.event_values..(index + 1) * arch.event_values];

            for ( target, value ) in slot.iter_mut().zip(values) { *target = value; }

        }

        let coverage = [
            f32::from(self.sample_seen > text_bytes || self.sample_seen > self.sample.len()),
            f32::from(self.response_seen > text_bytes || self.response_seen > self.response_sample.len()),
            f32::from(self.events_truncated || self.backend_events.len() > arch.event_count),
            f32::from(self.response_available),
        ];

        for ( target, value ) in input.coverage.iter_mut().zip(coverage) { *target = value; }

        input

    }

    pub fn summary ( input: &Input, arch: &Architecture ) -> Value {

        let text_bytes = arch.text_bytes;
        let events = input.event_values.chunks(arch.event_values.max(1)).filter(|row| row.first().is_some_and(|flag| *flag > 0.0)).count();

        json!({
            "schema"                : arch.name,
            "request_bytes"         : input.text[..text_bytes].iter().filter(|value| **value != 0).count(),
            "response_bytes"        : input.text[text_bytes..].iter().filter(|value| **value != 0).count(),
            "backend_events"        : events,
            "request_truncated"     : input.coverage.first().is_some_and(|value| *value > 0.0),
            "response_truncated"    : input.coverage.get(1).is_some_and(|value| *value > 0.0),
            "events_truncated"      : input.coverage.get(2).is_some_and(|value| *value > 0.0),
            "response_available"    : input.coverage.get(3).is_some_and(|value| *value > 0.0),
            "raw_content_persisted" : false,
        })

    }

    fn time ( value: u64 ) -> f32 {

        ((value.min(DAY_MS) as f64).ln_1p() / (DAY_MS as f64).ln_1p()) as f32

    }

}

impl Pending {

    pub fn submit ( self ) -> bool {

        let ( sample, sample_seen ) = self.request.borrow_mut().take();
        let ( response_sample, response_seen ) = self.response.borrow_mut().take();
        let response_available = self.analyser.scan_bytes().1 > 0;
        let ( backend_events, events_truncated ) = self.capture.with(|capture| capture.find(&self.id).map_or(( Vec::new(), false ), |journey| ( journey.backend_events.clone(), journey.truncated )));

        let envelope = Envelope {
            request_id         : self.id,
            route              : self.route,
            worker             : self.worker,
            admission          : self.admission,
            sample,
            sample_seen,
            response_sample,
            response_seen,
            response_available,
            events_truncated,
            outcome            : self.outcome.vector(),
            backend_events,
            started            : self.started,
        };

        self.analyser.submit(envelope)

    }

}
