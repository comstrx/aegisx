mod support;

use aegisx::config::{BackendConfig, ForwardAuth, PoolConfig, Route};
use support::{Http1, Origin, proxy};

fn guarded ( origin: std::net::SocketAddr, copy: &[&str] ) -> impl FnOnce(&mut aegisx::config::Config) {

    let copy: Vec<String> = copy.iter().map(|name| name.to_string()).collect();

    move |config| {

        config.pools.insert("auth".to_string(), PoolConfig { backends: vec![BackendConfig { address: origin.into(), ..BackendConfig::default() }], ..PoolConfig::default() });
        config.routes.push(Route { name: "app".to_string(), path: "/".to_string(), upstream: "default".to_string(), forward_auth: Some(ForwardAuth { upstream: "auth".to_string(), path: "/verify".to_string(), copy_headers: copy, ..ForwardAuth::default() }), ..Route::default() });

    }

}

#[test]
fn forward_auth_gates_requests_and_copies_granted_headers () {

    let origin = Origin::start();
    let running = proxy(origin.addr, guarded(origin.addr, &["x-user"]));
    let mut client = Http1::connect(running.addr());

    let denied = client.get("/private?x=1");

    assert_eq!(denied.status, 401);
    assert_eq!(denied.header("www-authenticate"), Some("Bearer realm=\"verify\""));
    assert_eq!(denied.text(), "who are you");

    let seen = origin.seen();

    assert_eq!(seen.len(), 1);
    assert_eq!(seen[0].path, "/verify");
    assert_eq!(seen[0].method, "GET");
    assert_eq!(seen[0].header("x-forwarded-method"), Some("GET"));
    assert_eq!(seen[0].header("x-forwarded-uri"), Some("/private?x=1"));
    assert!(seen[0].header("x-forwarded-host").is_some());

    let allowed = client.request("GET", "/private?x=1", &[( "Cookie", "session=ok" ), ( "X-User", "mallory" )], b"");

    assert_eq!(allowed.status, 200);
    assert_eq!(allowed.text(), "ok");

    let seen = origin.seen();

    assert_eq!(seen.len(), 2);
    assert_eq!(seen[0].path, "/verify");
    assert_eq!(seen[1].path, "/private?x=1");
    assert_eq!(seen[1].header("x-user"), Some("alice"));
    assert_eq!(seen[1].header("x-secret"), None);
    assert_eq!(seen[1].header("cookie"), Some("session=ok"));

    let posted = client.request("POST", "/echo", &[( "Cookie", "session=ok" ), ( "Content-Type", "text/plain" )], b"payload");

    assert_eq!(posted.status, 200);
    assert_eq!(posted.text(), "payload");

    let seen = origin.seen();

    assert_eq!(seen[0].path, "/verify");
    assert_eq!(seen[0].method, "GET");
    assert!(seen[0].body.is_empty());
    assert_eq!(seen[0].header("content-type"), None);
    assert_eq!(seen[0].header("x-forwarded-method"), Some("POST"));
    assert_eq!(seen[1].body, b"payload");

    running.stop().expect("stop");

}

#[test]
fn forward_auth_failures_become_502_and_configuration_is_validated () {

    let origin = Origin::start();
    let dead = support::free_port();
    let running = proxy(origin.addr, |config| {
        config.pools.insert("auth".to_string(), PoolConfig { backends: vec![BackendConfig { address: dead.into(), ..BackendConfig::default() }], ..PoolConfig::default() });
        config.routes.push(Route { name: "app".to_string(), path: "/".to_string(), upstream: "default".to_string(), forward_auth: Some(ForwardAuth { upstream: "auth".to_string(), path: "/verify".to_string(), ..ForwardAuth::default() }), ..Route::default() });
    });
    let mut client = Http1::connect(running.addr());

    assert_eq!(client.get("/x").status, 502);
    assert_eq!(origin.seen().len(), 0);

    running.stop().expect("stop");

    let source = |auth: &str| format!(r#"
        set_upstream("127.0.0.1:3000")
        add_upstream("auth", "127.0.0.1:3001")
        add_route {{ name = "app", path = "/", forward_auth = {{ {auth} }} }}
    "#);

    assert!(aegisx::config::Config::parse(&source(r#"upstream = "ghost", path = "/verify""#), "a.lua", std::path::Path::new("/tmp")).is_err());
    assert!(aegisx::config::Config::parse(&source(r#"upstream = "auth", path = "verify""#), "b.lua", std::path::Path::new("/tmp")).is_err());
    assert!(aegisx::config::Config::parse(&source(r#"upstream = "auth", path = "/verify", copy_headers = { "x-user" }"#), "c.lua", std::path::Path::new("/tmp")).is_ok());

}
