mod support;

use std::thread;
use std::time::{Duration, Instant};

use support::{Http1, Origin, proxy};

#[test]
fn forwards_and_keeps_both_sides_alive () {

    let origin = Origin::start();
    let running = proxy(origin.addr, |_| {});
    let mut client = Http1::connect(running.addr());

    for index in 0..200 {

        let reply = client.get(&format!("/ping/{index}"));

        assert_eq!(reply.status, 200);
        assert_eq!(reply.text(), "ok");

    }

    assert_eq!(origin.accepted(), 1);
    assert_eq!(origin.seen().len(), 200);

    running.stop().expect("stop");

}

#[test]
fn streams_request_body_to_origin_and_echoes_it () {

    let origin = Origin::start();
    let running = proxy(origin.addr, |config| config.limits.max_body_bytes = 4 * 1024 * 1024);
    let mut client = Http1::connect(running.addr());

    let body: Vec<u8> = (0..2 * 1024 * 1024).map(|index| (index % 251) as u8).collect();
    let reply = client.request("POST", "/echo", &[( "Content-Type", "application/octet-stream" )], &body);

    assert_eq!(reply.status, 200);
    assert_eq!(reply.body, body);

    let seen = origin.seen();

    assert_eq!(seen[0].method, "POST");
    assert_eq!(seen[0].body.len(), body.len());
    assert_eq!(seen[0].header("content-type"), Some("application/octet-stream"));

    running.stop().expect("stop");

}

#[test]
fn streams_chunked_response_from_origin () {

    let origin = Origin::start();
    let running = proxy(origin.addr, |_| {});
    let mut client = Http1::connect(running.addr());

    let started = Instant::now();
    let reply = client.get("/chunked/8/65536/20");

    assert_eq!(reply.status, 200);
    assert_eq!(reply.body.len(), 8 * 65536);
    assert!(reply.body.iter().all(|byte| *byte == b'c'));
    assert_eq!(reply.header("transfer-encoding"), Some("chunked"));
    assert!(started.elapsed() >= Duration::from_millis(7 * 20));

    let second = client.get("/after");

    assert_eq!(second.status, 200);

    running.stop().expect("stop");

}

#[test]
fn strips_hop_by_hop_headers_in_both_directions () {

    let origin = Origin::start();
    let running = proxy(origin.addr, |_| {});
    let mut client = Http1::connect(running.addr());

    let reply = client.request("GET", "/inspect", &[
        ( "Connection", "x-foo, keep-alive" ), ( "X-Foo", "secret" ), ( "Proxy-Connection", "keep-alive" ),
        ( "TE", "gzip, trailers" ), ( "Keep-Alive", "timeout=5" ), ( "X-Pass", "through" ),
    ], b"");

    assert_eq!(reply.status, 200);

    let seen = origin.seen();
    let request = &seen[0];

    assert_eq!(request.header("x-foo"), None);
    assert_eq!(request.header("proxy-connection"), None);
    assert_eq!(request.header("te"), Some("trailers"));
    assert_eq!(request.header("keep-alive"), None);
    assert_eq!(request.header("x-pass"), Some("through"));
    assert_eq!(request.header("host"), Some(origin.authority().as_str()));

    let reply = client.get("/hop");

    assert_eq!(reply.status, 200);
    assert_eq!(reply.header("x-bar"), None);
    assert_eq!(reply.header("keep-alive"), None);
    assert_eq!(reply.header("x-keep"), Some("yes"));

    let again = client.get("/still-alive");

    assert_eq!(again.status, 200);

    running.stop().expect("stop");

}

#[test]
fn rewrites_absolute_form_target_and_host () {

    let origin = Origin::start();
    let running = proxy(origin.addr, |_| {});
    let mut client = Http1::connect(running.addr());

    client.send(b"GET http://example.com/abs/path?q=1 HTTP/1.1\r\nHost: example.com\r\n\r\n");

    let reply = client.reply();

    assert_eq!(reply.status, 200);

    let seen = origin.seen();

    assert_eq!(seen[0].path, "/abs/path?q=1");
    assert_eq!(seen[0].header("host"), Some(origin.authority().as_str()));

    running.stop().expect("stop");

}

#[test]
fn returns_502_fast_when_upstream_is_down () {

    let closed = support::free_port();
    let running = proxy(closed, |_| {});
    let mut client = Http1::connect(running.addr());

    let started = Instant::now();
    let reply = client.get("/");

    assert_eq!(reply.status, 502);
    assert!(started.elapsed() < Duration::from_secs(1));

    let again = client.get("/");

    assert_eq!(again.status, 502);

    running.stop().expect("stop");

}

#[test]
fn returns_504_when_upstream_response_times_out () {

    let origin = Origin::start();
    let running = proxy(origin.addr, |config| config.limits.timeout_ms = 300);
    let mut client = Http1::connect(running.addr());

    let started = Instant::now();
    let reply = client.get("/slow/3000");

    assert_eq!(reply.status, 504);
    assert!(started.elapsed() < Duration::from_secs(2));

    let after = client.get("/fast");

    assert_eq!(after.status, 200);

    running.stop().expect("stop");

}

#[test]
fn recovers_when_origin_closes_every_connection () {

    let origin = Origin::start();
    let running = proxy(origin.addr, |_| {});
    let mut client = Http1::connect(running.addr());

    for _ in 0..100 {

        let reply = client.get("/close");

        assert_eq!(reply.status, 200);
        assert_eq!(reply.text(), "ok");

    }

    assert_eq!(origin.seen().len(), 100);

    running.stop().expect("stop");

}

#[test]
fn handles_many_concurrent_connections () {

    let origin = Origin::start();
    let running = proxy(origin.addr, |_| {});
    let addr = running.addr();

    let workers: Vec<_> = (0..32).map(|index| thread::spawn(move || {

        let mut client = Http1::connect(addr);

        for round in 0..100 {

            let reply = client.request("POST", "/echo", &[], format!("{index}-{round}").as_bytes());

            assert_eq!(reply.status, 200);
            assert_eq!(reply.text(), format!("{index}-{round}"));

        }

    })).collect();

    for worker in workers { worker.join().expect("client thread"); }

    assert_eq!(origin.seen().len(), 3200);
    assert!(origin.accepted() <= 32);

    running.stop().expect("stop");

}

#[test]
fn serves_http10_clients_and_closes_after_reply () {

    let origin = Origin::start();
    let running = proxy(origin.addr, |_| {});
    let mut client = Http1::connect(running.addr());

    client.send(b"GET /legacy HTTP/1.0\r\n\r\n");

    let reply = client.reply();

    assert_eq!(reply.status, 200);
    assert_eq!(reply.text(), "ok");
    assert!(client.try_reply().is_none());

    running.stop().expect("stop");

}

#[test]
fn stops_gracefully_while_a_request_is_in_flight () {

    let origin = Origin::start();
    let running = proxy(origin.addr, |_| {});
    let addr = running.addr();

    let slow = thread::spawn(move || {

        let mut client = Http1::connect(addr);

        client.get("/slow/700")

    });

    thread::sleep(Duration::from_millis(150));

    let started = Instant::now();

    running.stop().expect("stop");

    let reply = slow.join().expect("slow client");

    assert_eq!(reply.status, 200);
    assert!(started.elapsed() < Duration::from_secs(3));

}

#[test]
fn rejects_oversized_header_blocks () {

    let origin = Origin::start();
    let running = proxy(origin.addr, |config| config.server.max_headers = 32);
    let mut client = Http1::connect(running.addr());

    let mut raw = String::from("GET /many HTTP/1.1\r\nHost: test.local\r\n");

    for index in 0..64 { raw.push_str(&format!("X-H{index}: {index}\r\n")); }

    raw.push_str("\r\n");

    client.send(raw.as_bytes());

    let reply = client.try_reply();

    assert!(reply.as_ref().map(|reply| reply.status == 431).unwrap_or(true));
    assert!(origin.seen().is_empty());

    running.stop().expect("stop");

}

#[test]
fn upgrades_tunnel_bytes_in_both_directions () {

    let origin = Origin::start();
    let running = proxy(origin.addr, |_| {});
    let mut client = Http1::connect(running.addr());

    client.send(b"GET /upgrade HTTP/1.1\r\nHost: test.local\r\nConnection: Upgrade\r\nUpgrade: echo\r\n\r\n");

    let reply = client.reply();

    assert_eq!(reply.status, 101);
    assert_eq!(reply.header("upgrade"), Some("echo"));
    assert_eq!(reply.header("connection"), Some("upgrade"));
    assert_eq!(client.raw(b"ping", 4), b"ping");
    assert_eq!(client.raw(b"second message", 14), b"second message");
    assert_eq!(origin.seen()[0].header("upgrade"), Some("echo"));

    running.stop().expect("stop");

}

#[test]
fn trailers_cross_the_proxy_in_both_directions () {

    let origin = Origin::start();
    let direct = Http1::connect(origin.addr).request("GET", "/trailers", &[( "TE", "trailers" )], b"");

    assert_eq!(direct.text(), "trailed");
    assert_eq!(direct.trailers, vec![( "x-checksum".to_string(), "abc123".to_string() )], "origin itself did not send trailers: {:?}", direct.headers);

    let running = proxy(origin.addr, |_| {});
    let mut client = Http1::connect(running.addr());

    let reply = client.request("GET", "/trailers", &[( "TE", "trailers" )], b"");

    assert_eq!(reply.status, 200);
    assert_eq!(reply.text(), "trailed");
    assert_eq!(reply.trailers, vec![( "x-checksum".to_string(), "abc123".to_string() )]);

    client.send(b"POST /echo HTTP/1.1\r\nHost: test.local\r\nTransfer-Encoding: chunked\r\nTrailer: x-sig\r\n\r\n5\r\nhello\r\n0\r\nx-sig: deadbeef\r\n\r\n");

    let reply = client.reply();

    assert_eq!(reply.status, 200);
    assert_eq!(reply.text(), "hello");

    let seen = origin.seen();
    let post = seen.iter().find(|seen| seen.method == "POST").expect("post seen");

    assert_eq!(post.body, b"hello");
    assert_eq!(post.trailers, vec![( "x-sig".to_string(), "deadbeef".to_string() )]);

    running.stop().expect("stop");

}

#[test]
fn slow_request_bodies_are_cut_by_the_client_timeout () {

    let origin = Origin::start();
    let running = proxy(origin.addr, |config| config.limits.client_timeout_ms = 300);
    let mut client = Http1::connect(running.addr());
    let started = Instant::now();

    client.send(b"POST /echo HTTP/1.1\r\nHost: test.local\r\nContent-Length: 10\r\n\r\nabc");

    let reply = client.try_reply();

    assert!(reply.is_none() || reply.is_some_and(|reply| reply.status == 408), "slow upload was accepted");
    assert!(started.elapsed() < Duration::from_secs(3), "took {:?}", started.elapsed());

    running.stop().expect("stop");

}

#[test]
fn shared_accept_mode_serves_every_connection () {

    let origin = Origin::start();
    let running = proxy(origin.addr, |config| { config.runtime.accept = aegisx::http::server::Accept::Shared; config.runtime.workers = 3; });
    let addr = running.addr();

    let handles: Vec<_> = (0..24).map(|index| thread::spawn(move || {

        let mut client = Http1::connect(addr);
        let mut ok = 0;

        for round in 0..5 { if client.get(&format!("/shared/{index}/{round}")).status == 200 { ok += 1; } }

        ok

    })).collect();

    let served: usize = handles.into_iter().map(|handle| handle.join().expect("client")).sum();

    assert_eq!(served, 120);
    assert_eq!(origin.seen().len(), 120);

    running.stop().expect("stop");

}
