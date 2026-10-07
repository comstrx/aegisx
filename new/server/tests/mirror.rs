mod support;

use std::path::Path;
use std::thread;
use std::time::Duration;

use aegisx::config::{BackendConfig, Config, PoolConfig, Route};
use support::{Http1, Origin, Seen, free_port, proxy};

fn settled ( origin: &Origin, wanted: usize ) -> Vec<Seen> {

    let mut seen = Vec::new();

    for _ in 0..100 {

        seen.extend(origin.seen());

        if seen.len() >= wanted { break; }

        thread::sleep(Duration::from_millis(20));

    }

    seen

}

#[test]
fn mirrored_requests_reach_the_shadow_pool_without_touching_the_answer () {

    let origin = Origin::start();
    let shadow = Origin::start();
    let dead = free_port();
    let running = proxy(origin.addr, |config| {

        config.pools.insert("shadow".to_string(), PoolConfig { backends: vec![BackendConfig { address: shadow.addr.into(), ..BackendConfig::default() }], ..PoolConfig::default() });
        config.pools.insert("void".to_string(), PoolConfig { backends: vec![BackendConfig { address: dead.into(), ..BackendConfig::default() }], ..PoolConfig::default() });
        config.routes.push(Route { name: "api".to_string(), path: "/api".to_string(), upstream: "default".to_string(), mirror: Some("shadow".to_string()), strip_prefix: true, buffer_request: Some(true), ..Route::default() });
        config.routes.push(Route { name: "lost".to_string(), path: "/lost".to_string(), upstream: "default".to_string(), mirror: Some("void".to_string()), ..Route::default() });
        config.routes.push(Route { name: "all".to_string(), path: "/".to_string(), upstream: "default".to_string(), ..Route::default() });

    });
    let mut client = Http1::connect(running.addr());

    assert_eq!(client.get("/api/users?page=2").status, 200);
    assert_eq!(client.request("POST", "/api/orders", &[( "content-type", "application/json" )], br#"{"id":7}"#).status, 200);
    assert_eq!(client.get("/plain").status, 200);
    assert_eq!(client.get("/lost/x").status, 200);
    let seen = settled(&shadow, 2);
    let primary = origin.seen();

    assert_eq!(seen.len(), 2);
    assert_eq!(primary.len(), 4);

    let post = seen.iter().find(|entry| entry.method == "POST").expect("mirrored post");
    let get = seen.iter().find(|entry| entry.method == "GET").expect("mirrored get");

    assert_eq!(get.path, "/users?page=2");
    assert_eq!(post.path, "/orders");
    assert_eq!(post.body, br#"{"id":7}"#);
    assert_eq!(get.header("x-request-id"), primary[0].header("x-request-id"));

    running.stop().expect("stop");

}

#[test]
fn mirrors_must_name_a_known_pool () {

    let error = Config::parse(r#"
        set_upstream("127.0.0.1:3000")
        add_route { name = "api", path = "/", mirror = "nowhere" }
    "#, "mirror.lua", Path::new("/tmp")).map(|_| ()).map_err(|error| error.to_string());

    assert!(error.expect_err("unknown mirror pool").contains("nowhere"));

}
