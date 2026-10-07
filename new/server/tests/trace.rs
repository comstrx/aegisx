mod support;

use support::{Http1, Origin, proxy};

fn valid ( value: &str ) -> bool {

    let parts: Vec<&str> = value.split('-').collect();

    parts.len() == 4 && parts[0] == "00" && parts[1].len() == 32 && parts[2].len() == 16 && parts[3] == "01" && parts[1..3].iter().all(|part| part.bytes().all(|byte| byte.is_ascii_hexdigit()))

}

#[test]
fn a_trace_context_is_started_at_the_edge_and_kept_when_present () {

    let origin = Origin::start();
    let running = proxy(origin.addr, |config| config.identity.traceparent = true);
    let mut client = Http1::connect(running.addr());

    assert_eq!(client.get("/first").status, 200);
    assert_eq!(client.get("/second").status, 200);
    assert_eq!(client.request("GET", "/third", &[( "traceparent", "00-0af7651916cd43dd8448eb211c80319c-b7ad6b7169203331-01" )], b"").status, 200);

    let seen = origin.seen();
    let first = seen[0].header("traceparent").expect("generated trace context");
    let second = seen[1].header("traceparent").expect("generated trace context");

    assert!(valid(first), "{first}");
    assert!(valid(second), "{second}");
    assert_ne!(first, second);
    assert_eq!(seen[2].header("traceparent"), Some("00-0af7651916cd43dd8448eb211c80319c-b7ad6b7169203331-01"));

    running.stop().expect("stop");

    let silent = proxy(origin.addr, |_| {});

    assert_eq!(Http1::connect(silent.addr()).get("/plain").status, 200);
    assert_eq!(origin.seen()[0].header("traceparent"), None);

    silent.stop().expect("stop");

}

#[test]
fn finished_journeys_and_counters_reach_an_otlp_collector () {

    let origin = support::Origin::start();
    let collector = support::Origin::start();
    let running = support::proxy(origin.addr, |config| {

        config.telemetry.otlp = Some(format!("http://{}", collector.addr));
        config.telemetry.otlp_interval_ms = 200;
        config.telemetry.otlp_headers.insert("x-scope".to_string(), "edge".to_string());
        config.routes.push(aegisx::config::Route { name: "traced".to_string(), path: "/".to_string(), upstream: "default".to_string(), capture: true, ..aegisx::config::Route::default() });

    });

    assert_eq!(support::Http1::connect(running.addr()).get("/journey").status, 200);

    std::thread::sleep(std::time::Duration::from_millis(900));

    let batches = collector.seen();
    let traces = batches.iter().find(|seen| seen.path == "/v1/traces").expect("a trace batch");
    let text = String::from_utf8_lossy(&traces.body);

    assert_eq!(( traces.method.as_str(), traces.header("x-scope") ), ( "POST", Some("edge") ));
    assert!(text.contains("\"traceId\"") && text.contains("\"traced\"") && text.contains("\"forwarded\"") && text.contains("\"completed\""), "{text}");
    assert!(batches.iter().any(|seen| seen.path == "/v1/metrics" && String::from_utf8_lossy(&seen.body).contains("aegisx.requests")));

    running.stop().expect("stop");

    assert!(aegisx::config::Config::parse(r#"set_upstream("127.0.0.1:3000") set_telemetry { otlp = "not an address" }"#, "otlp.lua", std::path::Path::new("/tmp")).is_err());

}
