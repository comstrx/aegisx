mod support;

use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::{Duration, Instant};

use aegisx::app::Running;
use aegisx::config::{Config, ControlConfig, DecisionConfig, Route};
use serde_json::{Value, json};
use support::{Http1, Origin, Reply, free_port, proxy};

const ADMIN: &str = "test-admin-token-0123456789abcdef0123456789abcdef";

fn database ( tag: &str ) -> PathBuf {

    let path = std::env::temp_dir().join(format!("aegisx-decisions-{}-{tag}.db", std::process::id()));

    let _ = std::fs::remove_file(&path);
    let _ = std::fs::remove_file(path.with_extension("db-wal"));
    let _ = std::fs::remove_file(path.with_extension("db-shm"));

    path

}

fn decided ( upstream: SocketAddr, path: &Path, tune: impl FnOnce(&mut Config) ) -> Running {

    proxy(upstream, |config| {

        config.control = ControlConfig { enabled: true, listen: free_port(), token_env: "AEGISX_TEST_ADMIN_TOKEN".to_string(), ..ControlConfig::default() };
        config.decisions = DecisionConfig { enabled: true, path: path.to_path_buf(), ..DecisionConfig::default() };
        config.identity.trusted_peers = vec!["127.0.0.1/32".parse().expect("net")];
        config.identity.actor_header = Some("x-actor".to_string());
        config.routes.push(Route { name: "all".to_string(), path: "/".to_string(), upstream: "default".to_string(), capture: true, ..Route::default() });

        tune(config);

    })

}

fn call ( addr: SocketAddr, method: &str, path: &str, body: &[u8] ) -> Reply {

    let host = addr.to_string();
    let auth = format!("Bearer {ADMIN}");
    let mut headers = vec![( "Host", host.as_str() ), ( "Authorization", auth.as_str() )];

    if !body.is_empty() { headers.push(( "Content-Type", "application/json" )); }

    Http1::connect(addr).request(method, path, &headers, body)

}

fn json ( reply: &Reply ) -> Value {

    serde_json::from_slice(&reply.body).expect("json body")

}

fn actor_of ( control: SocketAddr, proxy_addr: SocketAddr, actor: &str ) -> ( String, String ) {

    assert_eq!(Http1::connect(proxy_addr).request("GET", "/probe", &[( "x-actor", actor )], b"").status, 200);

    let state = json(&call(control, "GET", "/api/v1/state", b""));
    let handle = state["telemetry"]["recent"].as_array().expect("recent").iter().find(|event| event["stage"] == "received").and_then(|event| event["details"]["actor"].as_str()).expect("actor").to_string();

    ( handle, state["config_version"].as_str().expect("version").to_string() )

}

fn get ( proxy_addr: SocketAddr, actor: &str ) -> u16 {

    Http1::connect(proxy_addr).request("GET", "/", &[( "x-actor", actor )], b"").status

}

#[test]
fn operator_blocks_apply_persist_across_restart_and_revoke () {

    let origin = Origin::start();
    let path = database("persist");
    let running = decided(origin.addr, &path, |_| {});
    let control = running.control_addr().expect("control");
    let ( actor, version ) = actor_of(control, running.addr(), "tenant-a");

    assert_eq!(actor.len(), 64);

    let block = json!({ "config_version": version, "route": "all", "actor": actor, "ttl_ms": 60_000, "reason": "manual review" });
    let reply = call(control, "POST", "/api/v1/blocks", block.to_string().as_bytes());

    assert_eq!(reply.status, 200, "{}", reply.text());

    let key = json(&reply)["key"].as_str().expect("key").to_string();

    assert_eq!(get(running.addr(), "tenant-a"), 403);
    assert_eq!(get(running.addr(), "tenant-b"), 200);

    let listed = json(&call(control, "GET", "/api/v1/decisions", b""));

    assert_eq!(listed["items"][0]["reason"], "manual review");
    assert_eq!(listed["items"][0]["source"], "operator");
    assert_eq!(listed["items"][0]["key"], key);

    let state = json(&call(control, "GET", "/api/v1/state", b""));

    assert_eq!(state["policies"]["decision_cache"], true);
    assert!(state["decisions"]["hits"].as_u64().expect("hits") >= 1);
    assert!(state["telemetry"]["recent"].as_array().expect("recent").iter().any(|event| event["stage"] == "decision" && event["details"]["reason"] == "manual review"));

    running.stop().expect("stop");

    let restarted = decided(origin.addr, &path, |_| {});

    assert_eq!(get(restarted.addr(), "tenant-a"), 403);
    assert_eq!(get(restarted.addr(), "tenant-b"), 200);

    let control = restarted.control_addr().expect("control");

    assert_eq!(call(control, "POST", "/api/v1/blocks/revoke", json!({ "key": key }).to_string().as_bytes()).status, 200);
    assert_eq!(get(restarted.addr(), "tenant-a"), 200);
    assert_eq!(json(&call(control, "GET", "/api/v1/decisions", b""))["items"], Value::Array(Vec::new()));
    assert_eq!(call(control, "POST", "/api/v1/blocks/revoke", br#"{"key":"zz"}"#).status, 400);

    restarted.stop().expect("stop");

}

#[test]
fn blocks_validate_scope_expire_and_respect_route_opt_out () {

    let origin = Origin::start();
    let path = database("scope");

    let running = decided(origin.addr, &path, |config| {

        config.decisions.deny_ttl_ms = 5_000;
        config.routes.push(Route { name: "open".to_string(), path: "/open".to_string(), upstream: "default".to_string(), decisions: Some(false), ..Route::default() });

    });

    let control = running.control_addr().expect("control");
    let ( actor, version ) = actor_of(control, running.addr(), "tenant-x");
    let post = |body: Value| call(control, "POST", "/api/v1/blocks", body.to_string().as_bytes());

    assert_eq!(post(json!({ "config_version": "stale", "route": "all", "actor": actor, "ttl_ms": 1000, "reason": "r" })).status, 409);
    assert_eq!(post(json!({ "config_version": version, "route": "missing", "actor": actor, "ttl_ms": 1000, "reason": "r" })).status, 404);
    assert_eq!(json(&post(json!({ "config_version": version, "route": "open", "actor": actor, "ttl_ms": 1000, "reason": "r" })))["error"], "decision_cache_disabled");
    assert_eq!(json(&post(json!({ "config_version": version, "route": "all", "actor": "nothex", "ttl_ms": 1000, "reason": "r" })))["error"], "invalid_actor");
    assert_eq!(json(&post(json!({ "config_version": version, "route": "all", "actor": actor, "ttl_ms": 10_000, "reason": "r" })))["error"], "invalid_block_scope");
    assert_eq!(json(&post(json!({ "config_version": version, "route": "all", "actor": actor, "ttl_ms": 1000, "reason": "" })))["error"], "invalid_block_scope");
    assert_eq!(post(json!({ "config_version": version, "route": "all", "actor": actor, "ttl_ms": 1000, "reason": "r", "extra": 1 })).status, 400);
    assert_eq!(post(json!({ "config_version": version, "route": "all", "actor": actor, "ttl_ms": 400, "reason": "short" })).status, 200);
    assert_eq!(get(running.addr(), "tenant-x"), 403);
    assert_eq!(Http1::connect(running.addr()).request("GET", "/open", &[( "x-actor", "tenant-x" )], b"").status, 200);

    thread::sleep(Duration::from_millis(500));

    assert_eq!(get(running.addr(), "tenant-x"), 200);

    running.stop().expect("stop");

}

#[test]
fn backend_block_header_restricts_the_actor_and_purge_keeps_bans () {

    let origin = Origin::start();
    let path = database("backend");

    let running = decided(origin.addr, &path, |config| config.identity.backend_block_header = Some("x-block".to_string()));

    let control = running.control_addr().expect("control");
    let reply = Http1::connect(running.addr()).request("GET", "/blockfor/30", &[( "x-actor", "tenant-z" )], b"");

    assert_eq!(reply.status, 200);
    assert!(reply.header("x-block").is_none());

    let deadline = Instant::now() + Duration::from_secs(3);

    while Instant::now() < deadline && get(running.addr(), "tenant-z") != 403 { thread::sleep(Duration::from_millis(20)); }

    assert_eq!(get(running.addr(), "tenant-z"), 403);
    assert_eq!(get(running.addr(), "tenant-y"), 200);

    let listed = json(&call(control, "GET", "/api/v1/decisions", b""));

    assert_eq!(listed["items"][0]["source"], "backend");

    assert_eq!(call(control, "POST", "/api/v1/cache/purge", br#"{"kind":"decisions"}"#).status, 200);
    assert_eq!(get(running.addr(), "tenant-z"), 403);
    assert!(json(&call(control, "GET", "/api/v1/state", b""))["decisions"]["generation"].as_u64().expect("generation") >= 1);

    running.stop().expect("stop");

}
