mod support;

use std::net::SocketAddr;

use aegisx::config::{BackendConfig, Config};
use support::{Http1, Origin, free_port, proxy};

fn pool ( config: &mut Config, backends: Vec<BackendConfig> ) {

    let entry = config.pools.entry("default".to_string()).or_default();

    entry.backends = backends;
    entry.options.attempts = 2;
    entry.options.retry_on = vec!["connect".to_string(), "error".to_string()];

}

fn backend ( addr: SocketAddr, backup: bool, down: bool ) -> BackendConfig {

    BackendConfig { address: addr.into(), backup, down, ..BackendConfig::default() }

}

#[test]
fn backup_backends_only_serve_when_every_primary_is_unavailable () {

    let primary = Origin::start();
    let spare = Origin::start();
    let dead = free_port();
    let running = proxy(primary.addr, |config| pool(config, vec![backend(dead, false, false), backend(primary.addr, false, false), backend(spare.addr, true, false)]));
    let mut client = Http1::connect(running.addr());

    for index in 0..12 { assert_eq!(client.get(&format!("/{index}")).status, 200, "request {index}"); }

    assert_eq!(primary.seen().len(), 12);
    assert_eq!(spare.seen().len(), 0);

    running.stop().expect("stop");

    let spare_only = Origin::start();
    let running = proxy(spare_only.addr, |config| pool(config, vec![backend(dead, false, false), backend(spare_only.addr, true, false)]));
    let mut client = Http1::connect(running.addr());

    assert_eq!(client.get("/first").status, 200);
    assert_eq!(client.get("/second").status, 200);
    assert_eq!(spare_only.seen().len(), 2);

    running.stop().expect("stop");

}

#[test]
fn down_backends_never_receive_traffic () {

    let live = Origin::start();
    let parked = Origin::start();
    let running = proxy(live.addr, |config| pool(config, vec![backend(parked.addr, false, true), backend(live.addr, false, false)]));
    let mut client = Http1::connect(running.addr());

    for index in 0..10 { assert_eq!(client.get(&format!("/{index}")).status, 200); }

    assert_eq!(live.seen().len(), 10);
    assert_eq!(parked.seen().len(), 0);

    running.stop().expect("stop");

    let parked_only = Origin::start();
    let running = proxy(parked_only.addr, |config| pool(config, vec![backend(parked_only.addr, false, true)]));
    let mut client = Http1::connect(running.addr());

    assert_eq!(client.get("/x").status, 503);
    assert_eq!(parked_only.seen().len(), 0);

    running.stop().expect("stop");

}
