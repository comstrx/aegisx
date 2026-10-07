mod support;

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use aegisx::app::{Content, Envelope, Input, Model};
use aegisx::config::{AnalysisConfig, AnalysisMode, ControlConfig, ModelSpec, Route};
use serde_json::Value;
use support::{Http1, Origin, Reply, free_port, proxy};

const ADMIN: &str = "test-admin-token-0123456789abcdef0123456789abcdef";

fn weights () -> PathBuf {

    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../model/weights")

}

fn fixtures ( name: &str ) -> Vec<Value> {

    serde_json::from_slice(&std::fs::read(weights().join(name)).expect("fixture file")).expect("fixture json")

}

fn call ( addr: SocketAddr, path: &str ) -> Reply {

    let host = addr.to_string();
    let auth = format!("Bearer {ADMIN}");

    Http1::connect(addr).request("GET", path, &[( "Host", host.as_str() ), ( "Authorization", auth.as_str() )], b"")

}

fn json ( reply: &Reply ) -> Value {

    serde_json::from_slice(&reply.body).expect("json body")

}

#[test]
fn content_extraction_matches_python_including_double_encoding () {

    let model = Model::load("lifecycle", &weights(), None, 1).expect("model");

    for case in fixtures("content-parity.json") {

        let sample: Vec<u8> = serde_json::from_value(case["sample"].clone()).expect("sample");
        let total = case["total"].as_u64().expect("total") as usize;
        let expected: Vec<f32> = serde_json::from_value(case["expected"].clone()).expect("expected");
        let actual = Content::extract(&sample, total, model.schema());

        assert_eq!(actual.len(), expected.len());

        for ( index, ( left, right ) ) in actual.iter().zip(&expected).enumerate() { assert!((left - right).abs() < 1e-6, "feature {index}: {left} vs {right}"); }

    }

}

#[test]
fn envelope_encoding_matches_python () {

    let model = Model::load("lifecycle", &weights(), None, 1).expect("model");
    let arch = &model.meta().architecture;

    for fixture in fixtures("lifecycle-parity.json") {

        let source = &fixture["envelope"];

        let envelope = Envelope {
            request_id         : Arc::from(source["request_id"].as_str().expect("id")),
            route              : Arc::from("fixture"),
            worker             : 0,
            admission          : serde_json::from_value(source["admission"].clone()).expect("admission"),
            sample             : serde_json::from_value(source["sample"].clone()).expect("sample"),
            sample_seen        : source["sample_seen"].as_u64().expect("seen") as usize,
            response_sample    : serde_json::from_value(source["response_sample"].clone()).expect("response sample"),
            response_seen      : source["response_seen"].as_u64().expect("response seen") as usize,
            response_available : source["response_available"].as_bool().expect("available"),
            events_truncated   : source["events_truncated"].as_bool().expect("truncated"),
            outcome            : serde_json::from_value(source["outcome"].clone()).expect("outcome"),
            backend_events     : serde_json::from_value(source["backend_events"].clone()).expect("events"),
            started            : Instant::now(),
        };

        let raw = envelope.raw(model.schema());
        let features = model.schema().normalize(&raw).expect("normalized");
        let actual = envelope.input(features, arch);
        let expected = Input::from_json(&fixture["expected"], arch).expect("expected input");

        assert_eq!(actual.text, expected.text);
        assert_eq!(actual.event_text, expected.event_text);
        assert_eq!(actual.coverage, expected.coverage);

        for ( left, right ) in actual.features.iter().chain(&actual.event_values).zip(expected.features.iter().chain(&expected.event_values)) {

            assert!((left - right).abs() < 1e-6, "{left} vs {right}");

        }

        assert!(model.predict(&actual).is_ok());

    }

}

#[test]
fn captured_requests_are_scored_in_the_background () {

    let origin = Origin::start();

    let running = proxy(origin.addr, |config| {

        config.models.insert("lifecycle".to_string(), ModelSpec { dir: weights(), features: None, threads: 1 });
        config.analysis = AnalysisConfig { mode: AnalysisMode::Observe, model: "lifecycle".to_string(), capacity: 16, ..AnalysisConfig::default() };
        config.routes.push(Route { name: "all".to_string(), path: "/".to_string(), upstream: "default".to_string(), capture: true, ..Route::default() });
        config.control = ControlConfig { enabled: true, listen: free_port(), token_env: "AEGISX_TEST_ADMIN_TOKEN".to_string(), ..ControlConfig::default() };

    });

    let control = running.control_addr().expect("control");
    let body = b"id=1%27+or+%271%27%3D%271+union+select+password+from+users--";
    let reply = Http1::connect(running.addr()).request("POST", "/echo?q=1", &[( "Content-Type", "application/x-www-form-urlencoded" )], body);
    let id = reply.header("x-request-id").expect("request id").to_string();

    assert_eq!(reply.status, 200);
    assert_eq!(reply.body, body);

    let deadline = Instant::now() + Duration::from_secs(5);
    let mut analyzed = None;

    while Instant::now() < deadline && analyzed.is_none() {

        let detail = json(&call(control, &format!("/api/v1/requests/{id}")));

        analyzed = detail["events"].as_array().and_then(|events| events.iter().find(|event| event["stage"] == "analyzed").cloned());

        if analyzed.is_none() { thread::sleep(Duration::from_millis(50)); }

    }

    let event = analyzed.expect("analyzed event");
    let risk = event["details"]["risk_score"].as_f64().expect("risk");

    assert!((0.0..=1.0).contains(&risk), "{risk}");
    assert_eq!(event["details"]["action"], "observe");
    assert_eq!(event["details"]["model"], "lifecycle");
    assert!(event["details"]["model_inputs"]["request_bytes"].as_u64().expect("request bytes") > body.len() as u64);
    assert!(event["details"]["model_inputs"]["response_bytes"].as_u64().expect("response bytes") > 0);
    assert_eq!(event["details"]["model_inputs"]["response_available"], true);

    let state = json(&call(control, "/api/v1/state"));

    assert_eq!(state["model"]["model_version"], "lifecycle-v9");
    assert_eq!(state["policies"]["model"], "observe");
    assert_eq!(state["configuration"]["feature_count"], 296);
    assert!(state["analysis"]["finished"].as_u64().expect("finished") >= 1);
    assert_eq!(state["analysis"]["failed"], 0);
    assert!(state["telemetry"]["recent"].as_array().expect("recent").iter().any(|event| event["stage"] == "analyzed"));

    running.stop().expect("stop");

}
