mod support;

use std::path::Path;

use aegisx::config::{Config, ListenConfig};
use support::{Http1, Origin, free_port, proxy, wait_for};

#[test]
fn extra_listeners_serve_the_same_routes () {

    let origin = Origin::start();
    let extra = free_port();
    let running = proxy(origin.addr, |config| config.listeners.push(ListenConfig { address: extra, ..ListenConfig::default() }));

    wait_for(extra);

    assert_eq!(Http1::connect(running.addr()).get("/main").status, 200);
    assert_eq!(Http1::connect(extra).get("/extra").status, 200);

    let seen = origin.seen();

    assert_eq!(seen.len(), 2);
    assert_eq!(seen[1].path, "/extra");
    assert_eq!(seen[1].header("x-forwarded-proto"), Some("http"));

    running.stop().expect("stop");

}

#[test]
fn redirect_listeners_send_every_request_to_https () {

    let origin = Origin::start();
    let plain = free_port();
    let running = proxy(origin.addr, |config| config.listeners.push(ListenConfig { address: plain, redirect: true, ..ListenConfig::default() }));

    wait_for(plain);

    let mut client = Http1::connect(plain);
    let moved = client.request("GET", "/docs/page?x=1", &[( "host", "example.test:8080" )], b"");

    assert_eq!(moved.status, 308);
    assert_eq!(moved.header("location"), Some("https://example.test/docs/page?x=1"));
    assert_eq!(client.request("POST", "/form", &[( "host", "example.test" )], b"a=1").status, 308);
    assert_eq!(origin.accepted(), 0);

    running.stop().expect("stop");

}

#[test]
fn listeners_are_validated () {

    let parsed = Config::parse(r#"
        set_upstream("127.0.0.1:3000")
        set_listen("127.0.0.1:8443")
        add_listen("127.0.0.1:8080")
        add_listen { address = "127.0.0.1:8081", redirect = true }
    "#, "listen.lua", Path::new("/tmp")).expect("parse");

    assert_eq!(parsed.listeners.len(), 2);
    assert!(!parsed.listeners[0].redirect);
    assert!(parsed.listeners[1].redirect);

    let parse = |source: &str| Config::parse(source, "listen.lua", Path::new("/tmp")).map(|_| ()).map_err(|error| error.to_string());

    assert!(parse(r#"set_upstream("127.0.0.1:3000") set_listen("127.0.0.1:8443") add_listen("127.0.0.1:8443")"#).expect_err("duplicate").contains("already in use"));
    assert!(parse(r#"set_upstream("127.0.0.1:3000") add_listen { address = "127.0.0.1:9443", tls = true }"#).expect_err("tls without certificate").contains("set_tls"));

}
