mod support;

use aegisx::config::{BackendConfig, Balance, Config, ForwardAuth, PoolConfig, Route};
use support::{Http1, Origin, proxy};

#[test]
fn forward_auth_identity_headers_never_come_from_the_client () {

    let origin = Origin::start();
    let auth = Origin::start();
    let running = proxy(origin.addr, |config| {

        config.pools.insert("auth".to_string(), PoolConfig { backends: vec![BackendConfig { address: auth.addr.into(), ..BackendConfig::default() }], ..PoolConfig::default() });
        config.routes.push(Route { name: "guarded".to_string(), path: "/".to_string(), upstream: "default".to_string(), forward_auth: Some(ForwardAuth { upstream: "auth".to_string(), path: "/verify".to_string(), copy_headers: vec!["x-user".to_string(), "x-role".to_string()], ..ForwardAuth::default() }), ..Route::default() });

    });
    let mut client = Http1::connect(running.addr());

    assert_eq!(client.request("GET", "/profile", &[( "cookie", "session=ok" ), ( "x-role", "admin" ), ( "x-other", "kept" )], b"").status, 200);

    let seen = origin.seen();

    assert_eq!(seen[0].header("x-role"), None);
    assert_eq!(seen[0].header("x-other"), Some("kept"));

    running.stop().expect("stop");

}

#[test]
fn underscore_headers_are_dropped_unless_allowed () {

    let origin = Origin::start();
    let strict = proxy(origin.addr, |_| {});

    assert_eq!(Http1::connect(strict.addr()).request("GET", "/", &[( "x_internal_user", "root" ), ( "x-plain", "1" )], b"").status, 200);

    strict.stop().expect("stop");

    let relaxed = proxy(origin.addr, |config| config.server.underscores_in_headers = true);

    assert_eq!(Http1::connect(relaxed.addr()).request("GET", "/", &[( "x_internal_user", "root" )], b"").status, 200);

    let seen = origin.seen();

    assert_eq!(seen[0].header("x_internal_user"), None);
    assert_eq!(seen[0].header("x-plain"), Some("1"));
    assert_eq!(seen[1].header("x_internal_user"), Some("root"));

    relaxed.stop().expect("stop");

}

#[test]
fn keep_alive_connections_are_retired_after_their_request_budget () {

    let origin = Origin::start();
    let running = proxy(origin.addr, |config| config.server.keepalive_requests = 3);
    let mut client = Http1::connect(running.addr());

    assert_eq!(client.get("/1").header("connection"), None);
    assert_eq!(client.get("/2").header("connection"), None);
    assert_eq!(client.get("/3").header("connection"), Some("close"));
    assert!(!client.try_send(b"GET /4 HTTP/1.1\r\nHost: test.local\r\n\r\n") || client.try_reply().is_none());
    assert_eq!(Http1::connect(running.addr()).get("/fresh").status, 200);

    running.stop().expect("stop");

}

#[test]
fn least_time_spreads_and_parses () {

    let first = Origin::start();
    let second = Origin::start();
    let running = proxy(first.addr, |config| {

        let pool = config.pools.entry("default".to_string()).or_default();

        pool.backends = [first.addr, second.addr].iter().map(|address| BackendConfig { address: (*address).into(), ..BackendConfig::default() }).collect();
        pool.options.policy = Balance::LeastTime;

    });
    let mut client = Http1::connect(running.addr());

    for index in 0..40 { assert_eq!(client.get(&format!("/{index}")).status, 200); }

    assert_eq!(first.seen().len() + second.seen().len(), 40);

    let parsed = Config::parse(r#"add_upstream("default", "127.0.0.1:3000") set_balancer("default", "least_time")"#, "balance.lua", std::path::Path::new("/tmp")).expect("parse");

    assert_eq!(parsed.pools["default"].options.policy, Balance::LeastTime);

    running.stop().expect("stop");

}

#[test]
fn clients_that_stop_reading_are_cut_after_the_send_timeout () {

    use std::io::{Read, Write};

    let origin = Origin::start();
    let root = std::env::temp_dir().join(format!("aegisx-stall-{}", std::process::id()));
    let size = 64usize << 20;

    std::fs::create_dir_all(&root).expect("root");
    std::fs::write(root.join("big.bin"), vec![7u8; size]).expect("big file");

    let running = proxy(origin.addr, |config| {

        config.server.send_timeout_ms = 300;
        config.routes.push(Route { name: "files".to_string(), path: "/".to_string(), root: Some(root.clone()), ..Route::default() });

    });
    let mut client = std::net::TcpStream::connect(running.addr()).expect("connect");

    client.write_all(b"GET /big.bin HTTP/1.1\r\nHost: test.local\r\n\r\n").expect("write");
    std::thread::sleep(std::time::Duration::from_millis(2_500));
    client.set_read_timeout(Some(std::time::Duration::from_secs(5))).expect("timeout");

    let mut received = 0usize;
    let mut buffer = vec![0u8; 1 << 16];

    loop {

        match client.read(&mut buffer) {
            Ok(0) | Err(_) => break,
            Ok(count) => received += count,
        }

    }

    assert!(received < size, "the stalled client still received the whole file");

    let _ = std::fs::remove_dir_all(&root);

    running.stop().expect("stop");

}

#[test]
fn shutdown_finishes_requests_in_flight_and_closes_keepalive_politely () {

    let origin = Origin::start();
    let running = proxy(origin.addr, |_| {});
    let addr = running.addr();

    let slow = std::thread::spawn(move || Http1::connect(addr).get("/slow/700").status);

    let polite = std::thread::spawn(move || {

        let mut client = Http1::connect(addr);
        let first = client.get("/one");

        std::thread::sleep(std::time::Duration::from_millis(350));

        let second = client.get("/two");

        ( first.status, first.header("connection").map(str::to_owned), second.status, second.header("connection").map(str::to_owned) )

    });

    std::thread::sleep(std::time::Duration::from_millis(150));

    let started = std::time::Instant::now();

    running.stop().expect("stop");

    assert!(started.elapsed() < std::time::Duration::from_millis(3_000), "drain took {:?}", started.elapsed());
    assert_eq!(slow.join().expect("slow"), 200);
    assert_eq!(polite.join().expect("polite"), ( 200, None, 200, Some("close".to_string()) ));

}

#[test]
fn connections_beyond_the_limit_are_closed_before_any_request () {

    let origin = Origin::start();
    let running = proxy(origin.addr, |config| {

        config.runtime.workers = 1;
        config.server.max_connections = 2;

    });

    std::thread::sleep(std::time::Duration::from_millis(150));

    let mut first = Http1::connect(running.addr());
    let mut second = Http1::connect(running.addr());

    assert_eq!(first.get("/one").status, 200);
    assert_eq!(second.get("/two").status, 200);

    let mut third = Http1::connect(running.addr());

    third.try_send(b"GET /three HTTP/1.1\r\nHost: test.local\r\n\r\n");

    assert!(third.try_reply().is_none());

    drop(first);

    std::thread::sleep(std::time::Duration::from_millis(200));

    assert_eq!(Http1::connect(running.addr()).get("/four").status, 200);
    assert_eq!(second.get("/five").status, 200);

    running.stop().expect("stop");

    assert!(aegisx::config::Config::parse(r#"set_server { max_connections = 99999999 }"#, "limit.lua", std::path::Path::new("/tmp")).is_err());

}

#[test]
fn swept_connections_give_their_slot_back () {

    let origin = Origin::start();
    let running = proxy(origin.addr, |config| {

        config.runtime.workers = 1;
        config.server.max_connections = 1;
        config.server.keepalive_timeout_ms = 200;

    });

    std::thread::sleep(std::time::Duration::from_millis(150));

    let mut idle = Http1::connect(running.addr());

    assert_eq!(idle.get("/one").status, 200);

    std::thread::sleep(std::time::Duration::from_millis(700));

    assert_eq!(Http1::connect(running.addr()).get("/two").status, 200);

    let started = std::time::Instant::now();

    running.stop().expect("stop");

    assert!(started.elapsed() < std::time::Duration::from_millis(3_000), "drain took {:?}", started.elapsed());

}
