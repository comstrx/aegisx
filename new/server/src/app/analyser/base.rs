use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::Instant;

use serde_json::{Value, json};

use crate::app::{Model, Telemetry};
use crate::config::{AnalysisMode, Config};
use crate::core::error::{AppError, AppResult};
use crate::core::log::debug;
use crate::core::queue::{Queue, Spec, Token};
use crate::core::time::Clock;
use super::arch::{Analyser, Envelope, Report};

impl Analyser {

    pub fn start ( config: &Config, telemetry: Arc<Telemetry> ) -> AppResult<Option<Arc<Self>>> {

        let settings = &config.analysis;

        if settings.mode == AnalysisMode::Off { return Ok(None); }

        Model::named(&settings.model).map_err(|_| AppError::config("set_analysis", format!("model `{}` is not configured", settings.model)))?;

        let report = Arc::new(Report::default());
        let spec = Spec { name: "aegisx-analysis", workers: settings.workers, capacity: settings.capacity, deadline_ms: settings.deadline_ms };

        let handler = {

            let report = report.clone();
            let telemetry = telemetry.clone();
            let model = settings.model.clone();

            move |envelope: Envelope, token: &Token| Self::run(&model, &telemetry, &report, envelope, token)

        };

        let queue = Queue::start(spec, handler)?;

        Ok(Some(Arc::new(Self { config: settings.clone(), queue, report, telemetry })))

    }

    pub fn submit ( &self, envelope: Envelope ) -> bool {

        match self.queue.submit(envelope) {
            Ok(()) => { self.report.submitted.fetch_add(1, Ordering::Relaxed); true }
            Err(envelope) => {

                self.report.dropped.fetch_add(1, Ordering::Relaxed);
                self.annotate(envelope.worker, &envelope.request_id, "analysis_skipped", Clock::elapsed_ms(envelope.started), json!({ "reason": "analysis_queue_full", "completed_request_unchanged": true }));

                false

            }
        }

    }

    pub fn model ( &self ) -> &str {

        &self.config.model

    }

    pub fn scan_bytes ( &self ) -> ( usize, usize ) {

        ( self.config.scan_bytes, self.config.response_scan_bytes )

    }

    pub fn state ( &self ) -> Value {

        let stats = self.queue.stats();
        let read = |counter: &std::sync::atomic::AtomicU64| counter.load(Ordering::Relaxed);

        json!({
            "processing"          : read(&stats.active),
            "journal_enabled"     : false,
            "journal_healthy"     : true,
            "journal_us"          : 0,
            "journal_failures"    : 0,
            "recovered"           : 0,
            "pressure_rejections" : read(&stats.rejected),
            "submitted"           : read(&self.report.submitted),
            "finished"            : read(&self.report.finished),
            "dropped"             : read(&self.report.dropped),
            "expired"             : read(&self.report.expired) + read(&stats.expired),
            "failed"              : read(&self.report.failed),
            "total_us"            : read(&stats.total_us),
            "last_us"             : read(&stats.last_us),
            "queued"              : self.queue.queued(),
            "capacity"            : self.config.capacity,
        })

    }

    pub fn stop ( &self ) {

        self.queue.stop();

    }

    fn run ( name: &str, telemetry: &Telemetry, report: &Report, envelope: Envelope, token: &Token ) -> AppResult<()> {

        let queue_ms = Clock::elapsed_ms(envelope.started);

        if token.cancelled() {

            report.expired.fetch_add(1, Ordering::Relaxed);
            Self::record(telemetry, envelope.worker, &envelope.request_id, "analysis_expired", queue_ms, json!({ "queue_ms": queue_ms }));

            return Ok(());

        }

        let started = Instant::now();

        let outcome = Model::named(name).and_then(|model| {

            let schema = model.schema();
            let raw = envelope.raw(schema);
            let features = schema.normalize(&raw).ok_or_else(|| AppError::invalid("analysis", "raw features are not finite and non-negative"))?;
            let input = envelope.input(features, &model.meta().architecture);
            let scores = model.predict(&input)?;

            Ok(( scores, Envelope::summary(&input, &model.meta().architecture) ))

        });

        let inference_us = Clock::elapsed_us(started);
        let elapsed_ms = Clock::elapsed_ms(envelope.started);

        match outcome {
            Ok(( scores, inputs )) => {

                report.finished.fetch_add(1, Ordering::Relaxed);

                Self::record(telemetry, envelope.worker, &envelope.request_id, "analyzed", elapsed_ms, json!({
                    "action"           : "observe",
                    "model"            : name,
                    "route"            : envelope.route,
                    "risk_score"       : scores.risk,
                    "component_scores" : { "risk": scores.risk, "content": scores.content, "journey": scores.journey },
                    "model_inputs"     : inputs,
                    "queue_ms"         : queue_ms,
                    "inference_us"     : inference_us,
                }));

                Ok(())

            }
            Err(error) => {

                report.failed.fetch_add(1, Ordering::Relaxed);
                debug!(request = %envelope.request_id, %error, "analysis failed");
                Self::record(telemetry, envelope.worker, &envelope.request_id, "analysis_failed", elapsed_ms, json!({ "reason": error.to_string() }));

                Err(error)

            }
        }

    }

    fn annotate ( &self, worker: usize, id: &str, stage: &'static str, elapsed_ms: u64, details: Value ) {

        Self::record(&self.telemetry, worker, id, stage, elapsed_ms, details);

    }

    fn record ( telemetry: &Telemetry, worker: usize, id: &str, stage: &'static str, elapsed_ms: u64, details: Value ) {

        if let Some(capture) = telemetry.captures.get(worker) { capture.with_mut(|capture| capture.annotate(id, stage, elapsed_ms, details)); }

    }

}
