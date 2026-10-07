mod support;

use std::net::SocketAddr;
use std::thread;
use std::time::Duration;

use aegisx::app::{Boot, Running};
use aegisx::config::{BackendConfig, Config, ControlConfig, Route};
use serde_json::Value;
use support::{Http1, Origin, Reply, free_port, proxy};

const ADMIN: &str = "test-admin-token-0123456789abcdef0123456789abcdef";
const BACKEND: &str = "test-backend-token-0123456789abcdef0123456789abcdef";

fn control ( listen: SocketAddr, panel_dir: Option<std::path::PathBuf> ) -> ControlConfig {

    ControlConfig {
        enabled           : true,
        listen,
        token_env         : "AEGISX_TEST_ADMIN_TOKEN".to_string(),
        backend_token_env : Some("AEGISX_TEST_BACKEND_TOKEN".to_string()),
        panel             : panel_dir.is_some(),
        panel_dir,
        ..ControlConfig::default()
    }

}

fn observed ( upstream: SocketAddr, tune: impl FnOnce(&mut Config) ) -> Running {

    proxy(upstream, |config| {

        config.control = control(free_port(), None);
        config.identity.trusted_peers = vec!["127.0.0.1/32".parse().expect("net")];

        tune(config);

    })

}

fn call ( addr: SocketAddr, token: &str, method: &str, path: &str, body: &[u8] ) -> Reply {

    let host = addr.to_string();
    let auth = format!("Bearer {token}");
    let mut headers = vec![( "Host", host.as_str() ), ( "Authorization", auth.as_str() )];

    if !body.is_empty() { headers.push(( "Content-Type", "application/json" )); }

    Http1::connect(addr).request(method, path, &headers, body)

}

fn json ( reply: &Reply ) -> Value {

    serde_json::from_slice(&reply.body).expect("json body")

}

#[test]
fn control_requires_bearer_host_and_same_origin () {

    let origin = Origin::start();
    let running = observed(origin.addr, |_| {});
    let addr = running.control_addr().expect("control");
    let host = addr.to_string();

    assert_eq!(Http1::connect(addr).request("GET", "/api/v1/state", &[( "Host", host.as_str() )], b"").status, 401);
    assert_eq!(call(addr, "wrong-token-0123456789abcdef0123456789abcdef", "GET", "/api/v1/state", b"").status, 401);
    assert_eq!(call(addr, BACKEND, "GET", "/api/v1/state", b"").status, 401);
    assert_eq!(Http1::connect(addr).request("GET", "/api/v1/state", &[( "Host", "evil.example:80" ), ( "Authorization", &format!("Bearer {ADMIN}") )], b"").status, 400);
    assert_eq!(Http1::connect(addr).request("GET", "/api/v1/state", &[( "Host", host.as_str() ), ( "Origin", "http://evil.example" ), ( "Authorization", &format!("Bearer {ADMIN}") )], b"").status, 403);
    assert_eq!(Http1::connect(addr).request("GET", "/api/v1/state", &[( "Host", host.as_str() ), ( "Origin", &format!("http://{host}") ), ( "Authorization", &format!("Bearer {ADMIN}") )], b"").status, 200);

    let reply = call(addr, ADMIN, "GET", "/api/v1/state", b"");

    assert_eq!(reply.status, 200);
    assert_eq!(reply.header("cache-control"), Some("no-store"));
    assert_eq!(reply.header("x-content-type-options"), Some("nosniff"));
    assert_eq!(call(addr, ADMIN, "GET", "/api/v1/unknown", b"").status, 404);
    assert_eq!(Http1::connect(addr).request("GET", "/", &[( "Host", host.as_str() )], b"").status, 404);

    running.stop().expect("stop");

}

#[test]
fn state_reflects_traffic_routes_and_upstreams () {

    let origin = Origin::start();

    let running = observed(origin.addr, |config| {

        config.routes.push(Route { name: "all".to_string(), path: "/".to_string(), upstream: "default".to_string(), ..Route::default() });
        config.routes.push(Route { name: "private".to_string(), path: "/internal".to_string(), deny: true, ..Route::default() });

    });

    let addr = running.control_addr().expect("control");
    let mut client = Http1::connect(running.addr());

    assert_eq!(client.get("/one").status, 200);
    assert_eq!(client.get("/two").status, 200);
    assert_eq!(client.get("/internal/secret").status, 403);

    let state = json(&call(addr, ADMIN, "GET", "/api/v1/state", b""));

    assert_eq!(state["telemetry"]["total"], 3);
    assert_eq!(state["telemetry"]["completed"], 2);
    assert_eq!(state["telemetry"]["blocked"], 1);
    assert_eq!(state["telemetry"]["active"], 0);
    assert_eq!(state["telemetry"]["latency_buckets"].as_array().map(Vec::len), Some(8));
    assert_eq!(state["upstreams"][0]["backends"][0]["address"], origin.addr.to_string());
    assert_eq!(state["upstreams"][0]["backends"][0]["healthy"], true);
    assert_eq!(state["configuration"]["threads"], 2);
    assert_eq!(state["policies"]["max_in_flight"], 4096);
    assert!(state["configuration"]["routes"].as_array().expect("routes").iter().any(|route| route["name"] == "private" && route["deny"] == true));
    assert!(state["resources"]["rss_bytes"].as_u64().is_some_and(|bytes| bytes > 0));
    assert!(state["uptime_ms"].as_u64().is_some());
    assert_eq!(state["journeys"]["total"], 0);
    assert_eq!(state["analysis"], Value::Null);
    assert_eq!(json(&call(addr, ADMIN, "GET", "/api/v1/decisions", b""))["items"], Value::Array(Vec::new()));
    assert_eq!(json(&call(addr, ADMIN, "GET", "/api/v1/cancellations", b""))["items"], Value::Array(Vec::new()));
    assert_eq!(call(addr, ADMIN, "GET", "/api/v1/contracts", b"").header("content-type"), Some("application/json"));

    running.stop().expect("stop");

}

#[test]
fn captured_routes_expose_journeys_and_events () {

    let origin = Origin::start();

    let running = observed(origin.addr, |config| {

        config.routes.push(Route { name: "all".to_string(), path: "/".to_string(), upstream: "default".to_string(), capture: true, ..Route::default() });

    });

    let addr = running.control_addr().expect("control");
    let reply = Http1::connect(running.addr()).get("/journey");
    let id = reply.header("x-request-id").expect("request id").to_string();

    assert_eq!(reply.status, 200);

    let state = json(&call(addr, ADMIN, "GET", "/api/v1/state", b""));
    let recent = state["telemetry"]["recent"].as_array().expect("recent");
    let stages: Vec<&str> = recent.iter().filter(|event| event["request_id"] == id).filter_map(|event| event["stage"].as_str()).collect();

    assert_eq!(stages, ["completed", "forwarded", "received"]);
    assert_eq!(recent[0]["details"]["status"], 200);
    assert_eq!(state["journeys"]["total"], 0);

    let detail = json(&call(addr, ADMIN, "GET", &format!("/api/v1/requests/{id}"), b""));
    let events = detail["events"].as_array().expect("events");

    assert_eq!(events.len(), 3);
    assert_eq!(events[0]["stage"], "received");
    assert_eq!(events[0]["details"]["route"], "all");
    assert_eq!(events[0]["details"]["path"], "/journey");
    assert_eq!(events[1]["details"]["backend"], origin.addr.to_string());
    assert_eq!(call(addr, ADMIN, "GET", "/api/v1/requests/not-a-uuid", b"").status, 400);
    assert_eq!(json(&call(addr, ADMIN, "GET", "/api/v1/requests/00000000-0000-4000-8000-000000000000", b""))["events"], Value::Array(Vec::new()));

    running.stop().expect("stop");

}

#[test]
fn backend_events_attach_only_to_active_journeys () {

    let origin = Origin::start();

    let running = observed(origin.addr, |config| {

        config.routes.push(Route { name: "all".to_string(), path: "/".to_string(), upstream: "default".to_string(), capture: true, ..Route::default() });

    });

    let addr = running.control_addr().expect("control");
    let proxy_addr = running.addr();
    let id = "7b4d1c2e-5a6f-4b8c-9d0e-1f2a3b4c5d6e";

    let worker = thread::spawn(move || Http1::connect(proxy_addr).request("GET", "/slow/700", &[( "x-request-id", id )], b"").status);

    thread::sleep(Duration::from_millis(200));

    let event = format!(r#"{{"request_id":"{id}","service":"billing","operation":"commit","state":"completed","duration_ms":12}}"#);

    assert_eq!(call(addr, ADMIN, "POST", "/api/v1/backend/events", event.as_bytes()).status, 401);
    assert_eq!(call(addr, BACKEND, "POST", "/api/v1/backend/events", event.as_bytes()).status, 202);
    assert_eq!(call(addr, BACKEND, "POST", "/api/v1/backend/events", br#"{"request_id":"nope","service":"billing","operation":"commit","state":"completed"}"#).status, 400);
    assert_eq!(call(addr, BACKEND, "POST", "/api/v1/backend/events", br#"{"request_id":"7b4d1c2e-5a6f-4b8c-9d0e-1f2a3b4c5d6e","service":"billing","operation":"commit","state":"completed","extra":1}"#).status, 400);

    let state = json(&call(addr, ADMIN, "GET", "/api/v1/state", b""));

    assert_eq!(state["journeys"]["total"], 1);
    assert_eq!(state["journeys"]["items"][0]["request_id"], id);
    assert_eq!(state["journeys"]["items"][0]["backend_events"][0]["operation"], "commit");
    assert_eq!(state["telemetry"]["active"], 1);
    assert_eq!(worker.join().expect("client"), 200);
    assert_eq!(call(addr, BACKEND, "POST", "/api/v1/backend/events", event.as_bytes()).status, 409);

    let detail = json(&call(addr, ADMIN, "GET", &format!("/api/v1/requests/{id}"), b""));
    let stages: Vec<&str> = detail["events"].as_array().expect("events").iter().filter_map(|event| event["stage"].as_str()).collect();

    assert_eq!(stages, ["received", "forwarded", "backend_reported", "completed"]);

    running.stop().expect("stop");

}

#[test]
fn panel_assets_and_bootstrap_are_served_with_policies () {

    let origin = Origin::start();
    let dir = std::env::temp_dir().join(format!("aegisx-panel-{}", std::process::id()));

    std::fs::create_dir_all(dir.join("_next/static")).expect("panel dir");
    std::fs::write(dir.join("index.html"), "<html>panel</html>").expect("index");
    std::fs::write(dir.join("about.html"), "<html>about</html>").expect("about");
    std::fs::write(dir.join("_next/static/app.js"), "console.log(1)").expect("script");

    let running = proxy(origin.addr, |config| config.control = control(free_port(), Some(dir.clone())));
    let addr = running.control_addr().expect("control");
    let host = addr.to_string();
    let get = |path: &str| Http1::connect(addr).request("GET", path, &[( "Host", host.as_str() )], b"");

    let index = get("/");

    assert_eq!(index.status, 200);
    assert_eq!(index.text(), "<html>panel</html>");
    assert_eq!(index.header("content-type"), Some("text/html; charset=utf-8"));
    assert_eq!(index.header("cache-control"), Some("no-store"));
    assert!(index.header("content-security-policy").is_some_and(|policy| policy.contains("frame-ancestors 'none'")));
    assert_eq!(get("/about").text(), "<html>about</html>");
    assert_eq!(get("/_next/static/app.js").header("cache-control"), Some("public, max-age=31536000, immutable"));
    assert_eq!(get("/_next/static/app.js").header("content-type"), Some("text/javascript; charset=utf-8"));
    assert_eq!(get("/missing").status, 404);
    assert_eq!(get("/../../etc/passwd").status, 404);
    assert_eq!(json(&get("/aegisx-bootstrap.json"))["api_prefix"], "/api/v1");
    assert_eq!(Http1::connect(addr).request("POST", "/", &[( "Host", host.as_str() )], b"x").status, 404);

    running.stop().expect("stop");

    let _ = std::fs::remove_dir_all(&dir);

}

#[test]
fn storage_backed_actions_report_409_and_bodies_are_capped () {

    let origin = Origin::start();
    let running = observed(origin.addr, |_| {});
    let addr = running.control_addr().expect("control");

    assert_eq!(json(&call(addr, ADMIN, "POST", "/api/v1/blocks", br#"{"config_version":"x","route":"all","actor":"a","ttl_ms":1,"reason":"r"}"#))["error"], "storage_disabled");
    assert_eq!(call(addr, ADMIN, "POST", "/api/v1/blocks/revoke", br#"{"key":"x"}"#).status, 409);
    assert_eq!(call(addr, ADMIN, "POST", "/api/v1/cancellations", br#"{"request_id":"x","route":"y"}"#).status, 409);
    assert_eq!(call(addr, ADMIN, "POST", "/api/v1/cache/purge", br#"{"kind":"all"}"#).status, 200);
    assert_eq!(json(&call(addr, ADMIN, "POST", "/api/v1/cache/purge", br#"{"kind":"everything"}"#))["error"], "unknown_cache");
    assert_eq!(call(addr, ADMIN, "POST", "/api/v1/cache/purge", br#"{"kind":"all","extra":true}"#).status, 400);
    assert_eq!(call(addr, ADMIN, "POST", "/api/v1/cache/purge", &vec![b'{'; 5_000]).status, 413);

    running.stop().expect("stop");

}

#[test]
fn check_and_reload_guard_the_control_surface () {

    let origin = Origin::start();
    let mut config = Config::default();

    config.set_upstream(origin.addr);
    config.control = ControlConfig { enabled: true, listen: free_port(), token_env: "AEGISX_TEST_MISSING_TOKEN".to_string(), ..ControlConfig::default() };

    assert!(Boot::check(&config).err().expect("missing token").to_string().contains("missing"));

    config.control.token_env = "AEGISX_TEST_ADMIN_TOKEN".to_string();
    config.control.listen = "10.0.0.1:9090".parse().expect("addr");

    assert!(Boot::check(&config).err().expect("non loopback").to_string().contains("loopback"));

    config.control.listen = free_port();
    config.control.backend_token_env = Some("AEGISX_TEST_ADMIN_TOKEN".to_string());

    assert!(Boot::check(&config).err().expect("same token").to_string().contains("differ"));

    let running = observed(origin.addr, |_| {});
    let mut next = Config { listen: running.addr(), ..Config::default() };

    next.set_upstream(origin.addr);
    next.runtime.workers = 2;
    next.runtime.pin = false;
    next.control = control(free_port(), None);

    assert!(running.reload(next).expect_err("control change").to_string().contains("restart"));

    running.stop().expect("stop");

}

#[test]
fn reload_endpoint_accepts_operator_requests () {

    let origin = Origin::start();
    let running = observed(origin.addr, |_| {});
    let control = running.control_addr().expect("control");

    assert_eq!(call(control, ADMIN, "POST", "/api/v1/reload", b"").status, 202);
    assert_eq!(call(control, BACKEND, "POST", "/api/v1/reload", b"").status, 401);

    running.stop().expect("stop");

}

#[test]
fn metrics_are_exposed_in_prometheus_text_format () {

    let origin = Origin::start();
    let running = observed(origin.addr, |_| {});
    let addr = running.control_addr().expect("control addr");

    assert_eq!(Http1::connect(running.addr()).get("/one").status, 200);
    assert_eq!(Http1::connect(running.addr()).get("/two").status, 200);

    let reply = call(addr, ADMIN, "GET", "/api/v1/metrics", b"");
    let text = reply.text();

    assert_eq!(reply.status, 200);
    assert_eq!(reply.header("content-type"), Some("text/plain; version=0.0.4; charset=utf-8"));
    assert!(text.contains("# TYPE aegisx_requests_total counter\naegisx_requests_total 2\n"), "{text}");
    assert!(text.contains("aegisx_request_duration_milliseconds_bucket{le=\"+Inf\"} 2\n"), "{text}");
    assert!(text.contains(&format!("aegisx_backend_up{{pool=\"default\",backend=\"{}\"}} 1\n", origin.addr)), "{text}");
    assert!(text.contains("aegisx_build_info{version="), "{text}");
    assert_eq!(call(addr, "wrong-token-0123456789abcdef0123456789abcdef", "GET", "/api/v1/metrics", b"").status, 401);

    running.stop().expect("stop");

}


#[test]
fn backend_counters_separate_responses_failures_and_retries () {

    let origin = Origin::start();
    let dead = free_port();

    let running = observed(origin.addr, |config| {

        let entry = config.pools.entry("default".to_string()).or_default();

        entry.backends = [dead, origin.addr].iter().map(|address| BackendConfig { address: (*address).into(), ..BackendConfig::default() }).collect();
        entry.options.attempts = 2;
        entry.options.max_fails = 100;

    });

    let addr = running.control_addr().expect("control addr");
    let mut client = Http1::connect(running.addr());

    for index in 0..6 { assert_eq!(client.get(&format!("/{index}")).status, 200); }

    let text = call(addr, ADMIN, "GET", "/api/v1/metrics", b"").text();
    let value = |name: &str, backend: SocketAddr| text.lines().find_map(|line| line.strip_prefix(&format!("aegisx_backend_{name}_total{{pool=\"default\",backend=\"{backend}\"}} "))).and_then(|value| value.parse::<u64>().ok()).unwrap_or_else(|| panic!("{name} missing in {text}"));

    assert_eq!(value("responses", origin.addr), 6, "{text}");
    assert_eq!(value("failures", origin.addr), 0, "{text}");
    assert_eq!(value("responses", dead), 0, "{text}");
    assert!(value("failures", dead) >= 1, "{text}");
    assert_eq!(value("retries", dead), value("failures", dead), "{text}");

    let state = json(&call(addr, ADMIN, "GET", "/api/v1/state", b""));
    let backends = state["upstreams"][0]["backends"].as_array().expect("backends");

    assert_eq!(backends[0]["failures"], value("failures", dead));
    assert_eq!(backends[1]["responses"], 6);

    running.stop().expect("stop");

}

#[test]
fn backends_join_and_leave_a_pool_through_the_api () {

    let first = Origin::start();
    let second = Origin::start();
    let running = observed(first.addr, |_| {});
    let addr = running.control_addr().expect("control addr");
    let mut client = Http1::connect(running.addr());

    for _ in 0..4 { assert_eq!(client.get("/before").status, 200); }

    assert_eq!(( first.seen().len(), second.seen().len() ), ( 4, 0 ));

    let joined = call(addr, ADMIN, "POST", "/api/v1/upstreams", format!(r#"{{"pool":"default","backend":{{"address":"{}","weight":1}}}}"#, second.addr).as_bytes());

    assert_eq!(joined.status, 200, "{}", joined.text());

    for _ in 0..20 { assert_eq!(client.get("/joined").status, 200); }

    assert!(second.seen().len() >= 5, "the new backend took no traffic");
    assert_eq!(json(&call(addr, ADMIN, "GET", "/api/v1/upstreams", b""))["items"][0]["backends"].as_array().expect("backends").len(), 2);

    let left = call(addr, ADMIN, "DELETE", "/api/v1/upstreams", format!(r#"{{"pool":"default","address":"{}"}}"#, first.addr).as_bytes());

    assert_eq!(left.status, 200, "{}", left.text());

    first.seen();

    for _ in 0..6 { assert_eq!(client.get("/left").status, 200); }

    assert_eq!(first.seen().len(), 0);
    assert_eq!(call(addr, ADMIN, "DELETE", "/api/v1/upstreams", format!(r#"{{"pool":"default","address":"{}"}}"#, second.addr).as_bytes()).status, 409);
    assert_eq!(call(addr, ADMIN, "POST", "/api/v1/upstreams", br#"{"pool":"nope","backend":{"address":"127.0.0.1:1"}}"#).status, 409);

    running.stop().expect("stop");

}

#[test]
fn runtime_backends_survive_a_file_reload_until_reset () {

    let first = Origin::start();
    let second = Origin::start();
    let mut seed = None;
    let running = observed(first.addr, |config| seed = Some(config.clone()));
    let seed = seed.expect("seed");
    let addr = running.control_addr().expect("control addr");
    let mut client = Http1::connect(running.addr());
    let edits = || json(&call(addr, ADMIN, "GET", "/api/v1/upstreams", b""))["edits"].clone();

    assert_eq!(call(addr, ADMIN, "POST", "/api/v1/upstreams", br#"{"pool":"default","backend":{"address":"127.0.0.1:1","weight":4000000000}}"#).status, 422);
    assert_eq!(edits(), 0, "a rejected edit is forgotten");
    assert_eq!(call(addr, ADMIN, "POST", "/api/v1/upstreams", format!(r#"{{"pool":"default","backend":{{"address":"{}"}}}}"#, second.addr).as_bytes()).status, 200);
    assert_eq!(call(addr, ADMIN, "DELETE", "/api/v1/upstreams", format!(r#"{{"pool":"default","address":"{}"}}"#, first.addr).as_bytes()).status, 200);
    assert_eq!(edits(), 2);

    running.reload(seed.clone()).expect("reload");
    first.seen();

    for _ in 0..6 { assert_eq!(client.get("/kept").status, 200); }

    assert_eq!(( first.seen().len(), second.seen().len() ), ( 0, 6 ), "the reload kept the runtime membership");
    assert_eq!(edits(), 2);
    assert_eq!(call(addr, ADMIN, "POST", "/api/v1/upstreams/reset", b"").status, 200);

    second.seen();

    for _ in 0..6 { assert_eq!(client.get("/reset").status, 200); }

    assert_eq!(( first.seen().len(), second.seen().len() ), ( 6, 0 ), "a reset returns to the file");
    assert_eq!(edits(), 0);

    running.stop().expect("stop");

}

#[test]
fn the_control_listener_keeps_plain_connections_when_the_proxy_listener_expects_a_preamble () {

    let origin = Origin::start();
    let admin = free_port();
    let running = proxy(origin.addr, |config| { config.control = control(admin, None); config.server.proxy_protocol = true; });

    assert_eq!(call(admin, ADMIN, "GET", "/api/v1/state", b"").status, 200);

    let mut client = Http1::connect(running.addr());

    client.send(b"PROXY TCP4 198.51.100.4 10.0.0.1 40000 80\r\n");

    assert_eq!(client.get("/").status, 200);

    running.stop().expect("stop");

}
