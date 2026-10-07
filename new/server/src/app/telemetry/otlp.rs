use std::sync::Arc;
use std::time::Instant;

use http::header::{CONTENT_TYPE, HOST, HeaderName, HeaderValue};
use http::{Method, Uri};
use serde_json::{Value, json};

use crate::app::{Journey, State};
use crate::core::log::debug;
use crate::core::net::Address;
use crate::core::rt::Rt;
use crate::core::sync::Watch;
use crate::core::time::Clock;
use crate::http::body::Body;
use crate::http::request::Req;
use crate::http::upstream::{Client, Protocol, Replay, Timing, Upstream};
use super::arch::Telemetry;

const TIMEOUT_MS: u64 = 5_000;

impl Telemetry {

    pub async fn export ( telemetry: Arc<Self>, state: State, mut stop: Watch ) {

        let Some(address) = telemetry.config.otlp.as_deref().and_then(|target| Address::parse(target.trim_start_matches("http://").trim_end_matches('/')).ok()) else { return; };
        let upstream = Upstream::new(address, Protocol::Http1);
        let client = Client::new(state.load().snapshot.config.client_settings());
        let extra: Vec<( HeaderName, HeaderValue )> = telemetry.config.otlp_headers.iter().filter_map(|( name, value )| Some(( HeaderName::from_bytes(name.as_bytes()).ok()?, HeaderValue::from_str(value).ok()? ))).collect();

        loop {

            tokio::select! {
                _ = Rt::sleep(telemetry.config.otlp_interval_ms) => {}
                _ = stop.wait() => break,
            }

            let journeys: Vec<Journey> = telemetry.captures.iter().flat_map(|capture| capture.with_mut(|capture| capture.drain())).collect();

            if !journeys.is_empty() { Self::post(&client, &upstream, "/v1/traces", &extra, Self::spans(&journeys)).await; }

            Self::post(&client, &upstream, "/v1/metrics", &extra, telemetry.points()).await;

        }

    }

    async fn post ( client: &Client, upstream: &Upstream, path: &'static str, extra: &[( HeaderName, HeaderValue )], payload: Value ) {

        let mut request = Req::new(Body::bytes(payload.to_string()));

        *request.method_mut() = Method::POST;
        *request.uri_mut() = Uri::from_static(path);
        request.headers_mut().insert(HOST, upstream.authority.clone());
        request.headers_mut().insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));

        for ( name, value ) in extra { request.headers_mut().insert(name.clone(), value.clone()); }

        match client.exchange(upstream, Timing { started: Instant::now(), timeout_ms: TIMEOUT_MS }, Replay::default(), Box::new(request), |_| None).await {
            Ok(( response, _ )) if response.status().is_success() => {}
            Ok(( response, _ )) => debug!(status = response.status().as_u16(), path, "otlp collector refused the batch"),
            Err(failure) => debug!(error = %failure.error, path, "otlp export failed"),
        }

    }

    fn resource () -> Value {

        json!({ "attributes": [{ "key": "service.name", "value": { "stringValue": "aegisx" } }] })

    }

    fn spans ( journeys: &[Journey] ) -> Value {

        let nanos = |millis: u64| millis.saturating_mul(1_000_000).to_string();
        let attributes = |details: &Value| details.as_object().map(|fields| fields.iter().map(|( key, value )| json!({ "key": key, "value": { "stringValue": value.as_str().map_or_else(|| value.to_string(), str::to_owned) } })).collect::<Vec<_>>()).unwrap_or_default();

        let spans: Vec<Value> = journeys.iter().filter_map(|journey| {

            let trace: String = journey.request_id.chars().filter(char::is_ascii_hexdigit).collect();

            if trace.len() != 32 { return None; }

            let ended = journey.events.last().map_or(journey.started_ms, |event| journey.started_ms + event.elapsed_ms);
            let failed = journey.events.last().is_some_and(|event| matches!(event.stage, "failed" | "rejected"));

            Some(json!({
                "traceId"           : trace,
                "spanId"            : &trace[16..],
                "name"              : &*journey.route,
                "kind"              : 2,
                "startTimeUnixNano" : nanos(journey.started_ms),
                "endTimeUnixNano"   : nanos(ended),
                "attributes"        : [{ "key": "aegisx.actor", "value": { "stringValue": journey.actor } }],
                "events"            : journey.events.iter().map(|event| json!({ "timeUnixNano": nanos(event.timestamp_ms), "name": event.stage, "attributes": attributes(&event.details) })).collect::<Vec<_>>(),
                "status"            : { "code": if failed { 2 } else { 1 } },
            }))

        }).collect();

        json!({ "resourceSpans": [{ "resource": Self::resource(), "scopeSpans": [{ "scope": { "name": "aegisx" }, "spans": spans }] }] })

    }

    fn points ( &self ) -> Value {

        let summary = self.summary();
        let now = Clock::now_ms().saturating_mul(1_000_000).to_string();
        let sum = |name: &str, value: u64| json!({ "name": name, "sum": { "aggregationTemporality": 2, "isMonotonic": true, "dataPoints": [{ "asInt": value.to_string(), "timeUnixNano": now }] } });

        let metrics = vec![
            sum("aegisx.requests", summary.total),
            sum("aegisx.requests.completed", summary.completed),
            sum("aegisx.requests.blocked", summary.blocked),
            sum("aegisx.requests.failed", summary.failed),
            sum("aegisx.request.bytes", summary.request_bytes),
            sum("aegisx.response.bytes", summary.response_bytes),
            json!({ "name": "aegisx.requests.active", "gauge": { "dataPoints": [{ "asInt": summary.active.to_string(), "timeUnixNano": now }] } }),
        ];

        json!({ "resourceMetrics": [{ "resource": Self::resource(), "scopeMetrics": [{ "scope": { "name": "aegisx" }, "metrics": metrics }] }] })

    }

}
