use std::sync::Arc;

use crate::app::{BackendEvent, Capture, Event, Journey};
use crate::config::TelemetryConfig;
use crate::config::base::consts::RECENT_EVENTS;
use crate::core::sync::Shared;
use super::arch::{BUCKETS, Outcome, Stats, Summary, Telemetry};

impl Telemetry {

    pub fn new ( workers: usize, config: &TelemetryConfig ) -> Self {

        let captures = (0..workers).map(|_| Shared::new(Capture::new(config.recent, config.journeys).exporting(config.otlp.is_some()))).collect();

        Self { config: config.clone(), workers: (0..workers).map(|_| Arc::new(Stats::default())).collect(), captures }

    }

    pub fn journey ( &self, id: &str ) -> Option<Journey> {

        self.captures.iter().find_map(|capture| capture.with(|capture| capture.find(id).cloned()))

    }

    pub fn backend ( &self, event: BackendEvent ) -> Option<()> {

        self.captures.iter().find_map(|capture| capture.with_mut(|capture| capture.backend(event.clone())))

    }

    pub fn observe ( &self ) -> ( Vec<Event>, Vec<Journey>, usize, u64 ) {

        let mut recent: Vec<Event> = Vec::new();
        let mut journeys: Vec<Journey> = Vec::new();
        let mut active = 0;
        let mut dropped = 0;

        for capture in &self.captures {

            capture.with(|capture| {

                recent.extend(capture.recent().cloned());
                journeys.extend(capture.active().take(RECENT_EVENTS).cloned());
                active += capture.active_count();
                dropped += capture.dropped();

            });

        }

        recent.sort_by(|left, right| right.timestamp_ms.cmp(&left.timestamp_ms).then(right.sequence.cmp(&left.sequence)));
        recent.truncate(self.config.recent);
        journeys.truncate(RECENT_EVENTS);

        ( recent, journeys, active, dropped )

    }

    pub fn summary ( &self ) -> Summary {

        let mut summary = Summary::default();

        for stats in &self.workers {

            summary.total += stats.total.get();
            summary.active += stats.active.get();
            summary.completed += stats.completed.get();
            summary.blocked += stats.blocked.get();
            summary.failed += stats.failed.get();
            summary.request_bytes += stats.request_bytes.get();
            summary.response_bytes += stats.response_bytes.get();

            for ( slot, bucket ) in summary.latency.iter_mut().zip(&stats.latency) { *slot += bucket.get(); }

        }

        summary

    }

}

impl Stats {

    pub fn settle ( &self ) {

        self.active.sub(1);

    }

    pub fn finish ( &self, outcome: Outcome, elapsed_ms: u64, request_bytes: u64, response_bytes: u64 ) {

        let counter = match outcome {
            Outcome::Completed => &self.completed,
            Outcome::Blocked => &self.blocked,
            Outcome::Failed => &self.failed,
        };

        counter.add(1);

        let bucket = BUCKETS.iter().position(|limit| elapsed_ms <= *limit).unwrap_or(BUCKETS.len());

        self.latency[bucket].add(1);
        self.request_bytes.add(request_bytes);
        self.response_bytes.add(response_bytes);

    }

}

impl Outcome {

    pub fn of ( status: u16 ) -> Self {

        match status {
            500.. => Self::Failed,
            _ => Self::Completed,
        }

    }

}
