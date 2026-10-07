mod support;

use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::mpsc::channel;
use std::thread;
use std::time::{Duration, Instant};

use aegisx::config::{BackendConfig, ListenConfig, PoolConfig, StreamConfig};
use aegisx::core::net::Wire;
use support::{Http1, Origin, free_port, proxy, wait_for};

#[test]
fn the_announced_source_becomes_the_client_address () {

    let origin = Origin::start();
    let extra = free_port();
    let running = proxy(origin.addr, |config| config.listeners.push(ListenConfig { address: extra, proxy_protocol: true, ..ListenConfig::default() }));

    wait_for(extra);

    let mut text = Http1::connect(extra);

    text.send(b"PROXY TCP4 203.0.113.9 10.0.0.1 54321 80\r\n");

    assert_eq!(text.get("/v1").status, 200);
    assert_eq!(text.get("/v1-again").status, 200);

    let source: SocketAddr = "[2001:db8::7]:4000".parse().expect("source");
    let destination: SocketAddr = "[2001:db8::1]:443".parse().expect("destination");
    let header = ppp::v2::Builder::with_addresses(ppp::v2::Version::Two | ppp::v2::Command::Proxy, ppp::v2::Protocol::Stream, ( source, destination )).build().expect("v2 header");
    let mut binary = Http1::connect(extra);

    binary.send(&header);

    assert_eq!(binary.get("/v2").status, 200);

    let mut plain = Http1::connect(running.addr());

    assert_eq!(plain.get("/plain").status, 200);

    let seen = origin.seen();

    assert_eq!(seen[0].header("x-forwarded-for"), Some("203.0.113.9"));
    assert_eq!(seen[1].header("x-forwarded-for"), Some("203.0.113.9"));
    assert_eq!(seen[2].header("x-forwarded-for"), Some("2001:db8::7"));
    assert_eq!(seen[3].header("x-forwarded-for"), Some("127.0.0.1"));

    running.stop().expect("stop");

}

#[test]
fn connections_without_a_header_are_closed () {

    let origin = Origin::start();
    let running = proxy(origin.addr, |config| config.server.proxy_protocol = true);
    let mut client = TcpStream::connect(running.addr()).expect("connect");

    client.set_read_timeout(Some(Duration::from_secs(5))).expect("timeout");
    client.write_all(b"GET / HTTP/1.1\r\nHost: test.local\r\n\r\n").expect("write");

    let started = Instant::now();
    let mut reply = Vec::new();
    let _ = client.read_to_end(&mut reply);

    assert!(reply.is_empty(), "the connection answered without a proxy protocol header: {:?}", String::from_utf8_lossy(&reply));
    assert!(started.elapsed() < Duration::from_secs(3), "the connection was not closed");
    assert_eq!(origin.accepted(), 0);

    let mut announced = Http1::connect(running.addr());

    announced.send(b"PROXY TCP4 198.51.100.4 10.0.0.1 40000 80\r\n");

    assert_eq!(announced.get("/").status, 200);
    assert_eq!(origin.seen()[0].header("x-forwarded-for"), Some("198.51.100.4"));

    running.stop().expect("stop");

}

#[test]
fn tcp_streams_announce_the_client_to_the_backend () {

    let origin = Origin::start();
    let listener = TcpListener::bind("127.0.0.1:0").expect("sink bind");
    let sink = listener.local_addr().expect("sink addr");
    let listen = free_port();
    let ( sender, receiver ) = channel();

    thread::spawn(move || {

        for stream in listener.incoming().flatten() {

            let mut stream = stream;
            let mut seen = Vec::new();
            let mut buffer = [0u8; 256];

            while !seen.ends_with(b"hello") {

                match stream.read(&mut buffer) {
                    Ok(0) | Err(_) => break,
                    Ok(count) => seen.extend_from_slice(&buffer[..count]),
                }

            }

            let _ = sender.send(seen);

        }

    });

    let running = proxy(origin.addr, |config| {
        config.pools.insert("sink".to_string(), PoolConfig { backends: vec![BackendConfig { address: sink.into(), ..BackendConfig::default() }], ..PoolConfig::default() });
        config.streams.push(StreamConfig { name: "sink".to_string(), listen, upstream: "sink".to_string(), proxy_protocol: Some(Wire::V1), ..StreamConfig::default() });
    });

    wait_for(listen);

    let mut client = TcpStream::connect(listen).expect("connect");
    let local = client.local_addr().expect("local");

    client.write_all(b"hello").expect("write");

    let seen = std::iter::from_fn(|| receiver.recv_timeout(Duration::from_secs(5)).ok()).find(|seen: &Vec<u8>| seen.ends_with(b"hello")).expect("backend bytes");

    assert_eq!(String::from_utf8_lossy(&seen), format!("PROXY TCP4 127.0.0.1 {} {} {}\r\nhello", listen.ip(), local.port(), listen.port()));

    running.stop().expect("stop");

}
