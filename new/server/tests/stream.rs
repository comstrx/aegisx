mod support;

use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::Path;
use std::thread;
use std::time::Duration;

use aegisx::config::{BackendConfig, Config, PoolConfig, StreamConfig};
use support::{Origin, free_port, proxy, wait_for};

fn echo () -> SocketAddr {

    let listener = TcpListener::bind("127.0.0.1:0").expect("echo bind");
    let addr = listener.local_addr().expect("echo addr");

    thread::spawn(move || {

        for stream in listener.incoming().flatten() {

            thread::spawn(move || {

                let mut stream = stream;
                let mut buffer = [0u8; 1024];

                loop {

                    match stream.read(&mut buffer) {
                        Ok(0) | Err(_) => break,
                        Ok(count) => { if stream.write_all(&buffer[..count]).is_err() { break; } }
                    }

                }

            });

        }

    });

    addr

}

#[test]
fn tcp_streams_are_relayed_to_a_balanced_backend_and_closed_when_idle () {

    let origin = Origin::start();
    let echo = echo();
    let listen = free_port();

    let running = proxy(origin.addr, |config| {
        config.pools.insert("echo".to_string(), PoolConfig { backends: vec![BackendConfig { address: echo.into(), ..BackendConfig::default() }], ..PoolConfig::default() });
        config.streams.push(StreamConfig { name: "echo".to_string(), listen, upstream: "echo".to_string(), idle_ms: 1_000, ..StreamConfig::default() });
    });

    wait_for(listen);

    let mut client = TcpStream::connect(listen).expect("connect");

    client.set_read_timeout(Some(Duration::from_secs(5))).expect("timeout");
    client.write_all(b"ping").expect("write");

    let mut reply = [0u8; 4];

    client.read_exact(&mut reply).expect("read");

    assert_eq!(&reply, b"ping");

    client.write_all(b"pong-again").expect("write");

    let mut reply = [0u8; 10];

    client.read_exact(&mut reply).expect("read");

    assert_eq!(&reply, b"pong-again");

    thread::sleep(Duration::from_millis(1_400));

    let mut tail = [0u8; 1];
    let closed = matches!(client.read(&mut tail), Ok(0) | Err(_));

    assert!(closed, "idle stream was not closed");

    let mut second = TcpStream::connect(listen).expect("connect again");

    second.set_read_timeout(Some(Duration::from_secs(5))).expect("timeout");
    second.write_all(b"x").expect("write");

    let mut reply = [0u8; 1];

    second.read_exact(&mut reply).expect("read");

    assert_eq!(&reply, b"x");
    assert_eq!(origin.seen().len(), 0);

    running.stop().expect("stop");

}

#[test]
fn stream_configuration_is_validated () {

    let parse = |source: &str| Config::parse(source, "stream.lua", Path::new("/tmp"));

    assert!(parse(r#"set_upstream("127.0.0.1:3000") add_upstream("db", "127.0.0.1:5433") add_stream { name = "pg", listen = "127.0.0.1:15432", upstream = "db" }"#).is_ok());
    assert!(parse(r#"set_upstream("127.0.0.1:3000") add_stream { name = "pg", listen = "127.0.0.1:15432", upstream = "ghost" }"#).is_err());
    assert!(parse(r#"set_upstream("127.0.0.1:3000") add_upstream("db", "127.0.0.1:5433") add_stream { listen = "127.0.0.1:15432", upstream = "db" }"#).is_err());
    assert!(parse(r#"set_listen("127.0.0.1:15432") set_upstream("127.0.0.1:3000") add_upstream("db", "127.0.0.1:5433") add_stream { name = "pg", listen = "127.0.0.1:15432", upstream = "db" }"#).is_err());
    assert!(parse(r#"set_upstream("127.0.0.1:3000") add_upstream("db", "127.0.0.1:5433") add_stream { name = "a", listen = "127.0.0.1:15432", upstream = "db" } add_stream { name = "b", listen = "127.0.0.1:15432", upstream = "db" }"#).is_err());
    assert!(parse(r#"set_upstream("127.0.0.1:3000") add_upstream("db", "127.0.0.1:5433") add_stream { name = "pg", listen = "127.0.0.1:15432", upstream = "db", idle_ms = 10 }"#).is_err());

    let config = parse(r#"set_upstream("127.0.0.1:3000") add_upstream("db", "127.0.0.1:5433") add_stream { name = "pg", listen = "127.0.0.1:15432", upstream = "db" }"#).expect("config");

    assert_eq!(config.streams[0].idle_ms, 600_000);

}

fn counter () -> ( SocketAddr, std::sync::Arc<std::sync::atomic::AtomicUsize> ) {

    let listener = TcpListener::bind("127.0.0.1:0").expect("sink bind");
    let addr = listener.local_addr().expect("sink addr");
    let hits = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let seen = hits.clone();

    thread::spawn(move || {

        for stream in listener.incoming().flatten() {

            let mut stream = stream;
            let mut buffer = [0u8; 64];

            if matches!(stream.read(&mut buffer), Ok(count) if count > 0) { seen.fetch_add(1, std::sync::atomic::Ordering::SeqCst); }

        }

    });

    ( addr, hits )

}

fn hello ( listen: SocketAddr, name: &str ) {

    let config = rustls::ClientConfig::builder().with_root_certificates(rustls::RootCertStore::empty()).with_no_client_auth();
    let mut session = rustls::ClientConnection::new(std::sync::Arc::new(config), rustls::pki_types::ServerName::try_from(name.to_string()).expect("name")).expect("session");
    let mut client = TcpStream::connect(listen).expect("connect");

    session.write_tls(&mut client).expect("client hello");
    client.set_read_timeout(Some(Duration::from_secs(2))).expect("timeout");

    let _ = client.read(&mut [0u8; 1]);

}

#[test]
fn tls_streams_are_routed_by_server_name_without_termination () {

    let origin = Origin::start();
    let ( fallback, fallback_hits ) = counter();
    let ( exact, exact_hits ) = counter();
    let ( wild, wild_hits ) = counter();
    let listen = free_port();

    let running = proxy(origin.addr, |config| {

        for ( name, addr ) in [( "fallback", fallback ), ( "exact", exact ), ( "wild", wild )] { config.pools.insert(name.to_string(), PoolConfig { backends: vec![BackendConfig { address: addr.into(), ..BackendConfig::default() }], ..PoolConfig::default() }); }

        config.streams.push(StreamConfig { name: "tls".to_string(), listen, upstream: "fallback".to_string(), sni: [( "app.example.test".to_string(), "exact".to_string() ), ( "*.wild.test".to_string(), "wild".to_string() )].into(), ..StreamConfig::default() });

    });

    wait_for(listen);

    hello(listen, "app.example.test");
    hello(listen, "api.wild.test");
    hello(listen, "elsewhere.test");

    let load = |hits: &std::sync::Arc<std::sync::atomic::AtomicUsize>| hits.load(std::sync::atomic::Ordering::SeqCst);

    assert_eq!(( load(&exact_hits), load(&wild_hits), load(&fallback_hits) ), ( 1, 1, 1 ));

    running.stop().expect("stop");

}

#[test]
fn udp_streams_relay_datagrams_both_ways_per_client () {

    use aegisx::config::{BackendConfig, PoolConfig, StreamConfig};

    let echo = std::net::UdpSocket::bind("127.0.0.1:0").expect("echo");
    let target = echo.local_addr().expect("echo addr");

    std::thread::spawn(move || {

        let mut buffer = [0u8; 2048];

        while let Ok(( size, peer )) = echo.recv_from(&mut buffer) {

            let mut reply = b"echo:".to_vec();

            reply.extend_from_slice(&buffer[..size]);

            if echo.send_to(&reply, peer).is_err() { break; }

        }

    });

    let origin = support::Origin::start();
    let listen = support::free_port();
    let running = support::proxy(origin.addr, |config| {

        config.pools.insert("dns".to_string(), PoolConfig { backends: vec![BackendConfig { address: target.into(), ..BackendConfig::default() }], ..PoolConfig::default() });
        config.streams.push(StreamConfig { name: "dns".to_string(), listen, upstream: "dns".to_string(), udp: true, ..StreamConfig::default() });

    });

    let ask = |text: &[u8]| {

        let client = std::net::UdpSocket::bind("127.0.0.1:0").expect("client");
        let mut buffer = [0u8; 2048];

        client.set_read_timeout(Some(std::time::Duration::from_secs(3))).expect("timeout");
        client.send_to(text, listen).expect("send");

        let first = client.recv(&mut buffer).map(|size| buffer[..size].to_vec()).expect("first reply");

        client.send_to(b"again", listen).expect("send again");

        ( first, client.recv(&mut buffer).map(|size| buffer[..size].to_vec()).expect("second reply") )

    };

    assert_eq!(ask(b"one"), ( b"echo:one".to_vec(), b"echo:again".to_vec() ));
    assert_eq!(ask(b"two"), ( b"echo:two".to_vec(), b"echo:again".to_vec() ));

    running.stop().expect("stop");

    assert!(aegisx::config::Config::parse(r#"set_upstream("127.0.0.1:3000") add_upstream("dns", "127.0.0.1:5353") add_stream { name = "dns", listen = "127.0.0.1:15353", upstream = "dns", udp = true }"#, "udp.lua", std::path::Path::new("/tmp")).is_ok());
    assert!(aegisx::config::Config::parse(r#"set_upstream("127.0.0.1:3000") add_upstream("dns", "127.0.0.1:5353") add_stream { name = "dns", listen = "127.0.0.1:15353", upstream = "dns", udp = true, proxy_protocol = "v1" }"#, "udp.lua", std::path::Path::new("/tmp")).is_err());

}

#[test]
fn stream_access_lists_and_connection_limits_refuse_clients () {

    use aegisx::config::Acl;

    let origin = Origin::start();
    let echo = echo();
    let ( open, closed, single ) = ( free_port(), free_port(), free_port() );

    let running = proxy(origin.addr, |config| {

        let stream = |name: &str, listen: SocketAddr| StreamConfig { name: name.to_string(), listen, upstream: "echo".to_string(), ..StreamConfig::default() };

        config.runtime.workers = 1;
        config.pools.insert("echo".to_string(), PoolConfig { backends: vec![BackendConfig { address: echo.into(), ..BackendConfig::default() }], ..PoolConfig::default() });
        config.streams.push(StreamConfig { acl: Acl { allow: vec!["127.0.0.0/8".parse().expect("net")], deny: Vec::new() }, ..stream("open", open) });
        config.streams.push(StreamConfig { acl: Acl { allow: Vec::new(), deny: vec!["127.0.0.1/32".parse().expect("net")] }, ..stream("closed", closed) });
        config.streams.push(StreamConfig { max_connections: 1, ..stream("single", single) });

    });

    for listen in [open, closed, single] { wait_for(listen); }

    thread::sleep(Duration::from_millis(300));

    let echoed = |listen: SocketAddr| {

        let mut client = TcpStream::connect(listen).expect("connect");
        let mut reply = [0u8; 4];

        client.set_read_timeout(Some(Duration::from_secs(3))).expect("timeout");

        let alive = client.write_all(b"ping").is_ok() && client.read_exact(&mut reply).is_ok() && &reply == b"ping";

        ( alive, client )

    };

    assert!(echoed(open).0, "an allowed client is relayed");
    assert!(!echoed(closed).0, "a denied client is dropped");

    let ( first, held ) = echoed(single);

    assert!(first);
    assert!(!echoed(single).0, "the limit refuses a second connection");

    drop(held);
    thread::sleep(Duration::from_millis(200));

    assert!(echoed(single).0, "and frees its place when the first one ends");

    running.stop().expect("stop");

}

