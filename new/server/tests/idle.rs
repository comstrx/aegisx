mod support;

use std::io::Write;
use std::net::TcpStream;
use std::thread;
use std::time::Duration;

use support::{Http1, Origin, proxy};

#[test]
fn idle_keepalive_connections_are_closed_by_the_sweeper () {

    let origin = Origin::start();
    let running = proxy(origin.addr, |config| { config.server.keepalive_timeout_ms = 400; config.server.header_timeout_ms = 400; });
    let mut client = Http1::connect(running.addr());

    assert_eq!(client.get("/first").status, 200);

    thread::sleep(Duration::from_millis(150));

    assert_eq!(client.get("/second").status, 200);

    thread::sleep(Duration::from_millis(1_200));

    client.send(b"GET /third HTTP/1.1\r\nHost: test.local\r\n\r\n");

    assert!(client.try_reply().is_none(), "idle connection survived the keepalive timeout");

    let mut silent = TcpStream::connect(running.addr()).expect("connect");

    silent.set_read_timeout(Some(Duration::from_secs(3))).expect("timeout");
    thread::sleep(Duration::from_millis(1_200));

    let outcome = silent.write_all(b"GET /late HTTP/1.1\r\nHost: test.local\r\n\r\n").and_then(|()| { let mut buffer = [0u8; 16]; std::io::Read::read(&mut silent, &mut buffer) });

    assert!(matches!(outcome, Ok(0) | Err(_)), "connection that never sent a request stayed open");

    running.stop().expect("stop");

}

#[test]
fn busy_connections_are_never_swept () {

    let origin = Origin::start();
    let running = proxy(origin.addr, |config| { config.server.keepalive_timeout_ms = 300; config.server.header_timeout_ms = 300; config.limits.timeout_ms = 5_000; });
    let mut client = Http1::connect(running.addr());

    let reply = client.get("/slow/1500");

    assert_eq!(reply.status, 200);
    assert_eq!(client.get("/after").status, 200);

    running.stop().expect("stop");

}
