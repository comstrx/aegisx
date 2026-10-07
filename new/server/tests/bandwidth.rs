mod support;

use std::time::Instant;

use aegisx::config::Route;
use support::{Http1, Origin, proxy};

#[test]
fn bandwidth_limits_pace_response_bodies_after_the_free_prefix () {

    let origin = Origin::start();
    let running = proxy(origin.addr, |config| {
        config.routes.push(Route { name: "slow".to_string(), path: "/slow".to_string(), upstream: "default".to_string(), strip_prefix: true, bandwidth: Some(40_000), ..Route::default() });
        config.routes.push(Route { name: "head".to_string(), path: "/head".to_string(), upstream: "default".to_string(), strip_prefix: true, bandwidth: Some(40_000), bandwidth_after: Some(100_000), ..Route::default() });
        config.routes.push(Route { name: "free".to_string(), path: "/".to_string(), upstream: "default".to_string(), ..Route::default() });
    });
    let mut client = Http1::connect(running.addr());

    let started = Instant::now();
    let reply = client.get("/slow/large/120000");
    let paced = started.elapsed();

    assert_eq!(reply.status, 200);
    assert_eq!(reply.body.len(), 120_000);
    assert!(paced.as_millis() >= 2_000 && paced.as_millis() < 6_000, "paced body took {paced:?}");

    let started = Instant::now();
    let reply = client.get("/head/large/120000");
    let partly = started.elapsed();

    assert_eq!(reply.body.len(), 120_000);
    assert!(partly.as_millis() >= 300 && partly.as_millis() < 2_000, "partly paced body took {partly:?}");

    let started = Instant::now();
    let reply = client.get("/large/120000");

    assert_eq!(reply.body.len(), 120_000);
    assert!(started.elapsed().as_millis() < 1_000);

    running.stop().expect("stop");

}

#[test]
fn bandwidth_configuration_is_bounded () {

    let bad = aegisx::config::Config::parse(r#"set_upstream("127.0.0.1:3000") set_limits { bandwidth = 2000000000000000 }"#, "bw.lua", std::path::Path::new("/tmp"));

    assert!(bad.is_err());

    let ok = aegisx::config::Config::parse(r#"set_upstream("127.0.0.1:3000") set_limits { bandwidth = 1048576, bandwidth_after = 65536 } add_route { name = "r", path = "/", bandwidth = 0 }"#, "bw.lua", std::path::Path::new("/tmp")).expect("config");

    assert_eq!(ok.limits.bandwidth, 1_048_576);
    assert_eq!(ok.routes[0].bandwidth, Some(0));

}
