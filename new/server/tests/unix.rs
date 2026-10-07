#![cfg(unix)]

mod support;

use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::time::Duration;

use aegisx::config::{BackendConfig, Config, PoolConfig};
use aegisx::core::net::Address;
use support::{Http1, Origin, proxy};

fn socket ( name: &str ) -> PathBuf {

    let dir = std::env::temp_dir().join(format!("aegisx-unix-{}", std::process::id()));

    std::fs::create_dir_all(&dir).expect("socket dir");

    dir.join(format!("{name}.sock"))

}

#[test]
fn forwards_to_a_unix_socket_upstream () {

    let path = socket("upstream");
    let origin = Origin::start_unix(&path);
    let log = socket("upstream").with_extension("log");
    let running = proxy(origin.addr, |config| {

        config.pools.insert("default".to_string(), PoolConfig { backends: vec![BackendConfig { address: Address::parse(&format!("unix:{}", path.display())).expect("address"), ..BackendConfig::default() }], ..PoolConfig::default() });
        config.access.path = log.clone();

    });
    let mut client = Http1::connect(running.addr());

    for index in 0..20 {

        let reply = client.get(&format!("/ping/{index}"));

        assert_eq!(reply.status, 200);
        assert_eq!(reply.text(), "ok");

    }

    assert_eq!(origin.seen().len(), 20);
    assert_eq!(origin.accepted(), 1);

    running.stop().expect("stop");

    let line = std::fs::read_to_string(&log).expect("access log");

    assert!(line.contains(&format!(" unix:{} ", path.display())), "{line}");

}

#[test]
fn listens_on_a_unix_socket_alongside_tcp () {

    let origin = Origin::start();
    let path = socket("listen");
    let running = proxy(origin.addr, |config| { config.listen_unix = Some(path.display().to_string()); config.runtime.workers = 2; });

    assert_eq!(Http1::connect(running.addr()).get("/tcp").status, 200);

    for round in 0..6 {

        let mut stream = UnixStream::connect(&path).expect("unix connect");

        stream.set_read_timeout(Some(Duration::from_secs(5))).expect("timeout");
        stream.write_all(format!("GET /unix/{round} HTTP/1.1\r\nHost: test.local\r\nConnection: close\r\n\r\n").as_bytes()).expect("send");

        let mut reply = Vec::new();

        stream.read_to_end(&mut reply).expect("read");

        let text = String::from_utf8_lossy(&reply);

        assert!(text.starts_with("HTTP/1.1 200"), "{text}");
        assert!(text.ends_with("ok"), "{text}");

    }

    assert_eq!(origin.seen().len(), 7);

    running.stop().expect("stop");

}

#[test]
fn tls_over_unix_upstreams_is_rejected_at_validation () {

    let mut config = Config { listen: support::free_port(), ..Config::default() };

    config.pools.insert("default".to_string(), PoolConfig { backends: vec![BackendConfig { address: Address::parse("unix:/tmp/x.sock").expect("address"), tls: true, server_name: "x".to_string(), ..BackendConfig::default() }], ..PoolConfig::default() });

    let error = config.validate().expect_err("tls over unix must fail").to_string();

    assert!(error.contains("unix"), "{error}");

}
