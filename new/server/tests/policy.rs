mod support;

use std::sync::atomic::Ordering;
use std::thread;
use std::time::{Duration, Instant};

use aegisx::config::{Balance, BackendConfig, Config, HealthConfig, Route};
use support::{Http1, Origin, free_port, proxy};

fn pool ( config: &mut Config, name: &str, backends: &[( std::net::SocketAddr, u32 )] ) {

    let entry = config.pools.entry(name.to_string()).or_default();

    for ( address, weight ) in backends { entry.backends.push(BackendConfig { address: (*address).into(), weight: *weight, ..BackendConfig::default() }); }

}

fn route <'a> ( config: &'a mut Config, name: &str, path: &str, upstream: &str ) -> &'a mut Route {

    config.routes.push(Route { name: name.to_string(), path: path.to_string(), upstream: upstream.to_string(), ..Route::default() });

    config.routes.last_mut().expect("route")

}

#[test]
fn routes_by_host_and_path_to_different_pools () {

    let a = Origin::start();
    let b = Origin::start();

    let running = proxy(a.addr, |config| {

        pool(config, "b", &[( b.addr, 1 )]);
        route(config, "a", "/a", "default");
        route(config, "b", "/", "b").host = Some("b.local".to_string());
        route(config, "root", "/", "default");

    });

    let mut client = Http1::connect(running.addr());

    assert_eq!(client.request("GET", "/a/x", &[( "Host", "whatever" )], b"").status, 200);
    assert_eq!(client.request("GET", "/", &[( "Host", "b.local" )], b"").status, 200);
    assert_eq!(client.request("GET", "/zzz", &[( "Host", "other" )], b"").status, 200);

    let seen_a = a.seen();
    let seen_b = b.seen();

    assert_eq!(seen_a.len(), 2);
    assert_eq!(seen_a[0].path, "/a/x");
    assert_eq!(seen_a[1].path, "/zzz");
    assert_eq!(seen_b.len(), 1);
    assert_eq!(seen_b[0].path, "/");

    running.stop().expect("stop");

}

#[test]
fn strips_prefixes_and_injects_headers () {

    let origin = Origin::start();

    let running = proxy(origin.addr, |config| {

        let api = route(config, "api", "/api", "default");

        api.strip_prefix = true;
        api.request_headers.insert("x-env".to_string(), "prod".to_string());
        api.response_headers.insert("x-served".to_string(), "aegisx".to_string());

    });

    let mut client = Http1::connect(running.addr());
    let reply = client.get("/api/items?q=1");

    assert_eq!(reply.status, 200);
    assert_eq!(reply.header("x-served"), Some("aegisx"));

    let seen = origin.seen();

    assert_eq!(seen[0].path, "/items?q=1");
    assert_eq!(seen[0].header("x-env"), Some("prod"));

    assert_eq!(client.get("/api").status, 200);
    assert_eq!(origin.seen()[0].path, "/");

    running.stop().expect("stop");

}

#[test]
fn denies_and_rejects_unmatched_requests () {

    let origin = Origin::start();

    let running = proxy(origin.addr, |config| {

        route(config, "api", "/api", "default");
        route(config, "secret", "/secret", "default").deny = true;

    });

    let mut client = Http1::connect(running.addr());

    assert_eq!(client.get("/api/ok").status, 200);
    assert_eq!(client.get("/secret/x").status, 403);
    assert_eq!(client.get("/elsewhere").status, 404);
    assert_eq!(client.get("/x/../api").status, 400);
    assert_eq!(origin.seen().len(), 1);

    running.stop().expect("stop");

}

#[test]
fn enforces_body_limits () {

    let origin = Origin::start();
    let running = proxy(origin.addr, |config| config.limits.max_body_bytes = 1024);
    let mut client = Http1::connect(running.addr());

    let reply = client.request("POST", "/echo", &[], &vec![b'x'; 2048]);

    assert_eq!(reply.status, 413);
    assert!(origin.seen().is_empty());

    let small = client.request("POST", "/echo", &[], &vec![b'y'; 512]);

    assert_eq!(small.status, 200);
    assert_eq!(small.body.len(), 512);

    let mut streaming = Http1::connect(running.addr());

    streaming.send(b"POST /echo HTTP/1.1\r\nHost: t\r\nTransfer-Encoding: chunked\r\n\r\n");

    for _ in 0..8 { streaming.send(format!("{:x}\r\n{}\r\n", 512, "z".repeat(512)).as_bytes()); }

    streaming.send(b"0\r\n\r\n");

    let outcome = streaming.try_reply();

    assert!(outcome.as_ref().map(|reply| reply.status == 413 || reply.status == 502).unwrap_or(true));
    assert!(origin.seen().iter().all(|seen| seen.body.len() < 4096));

    running.stop().expect("stop");

}

#[test]
fn request_ids_are_generated_propagated_and_echoed () {

    let origin = Origin::start();
    let running = proxy(origin.addr, |_| {});
    let mut client = Http1::connect(running.addr());

    let reply = client.request("GET", "/", &[( "X-Request-Id", "spoofed" )], b"");
    let echoed = reply.header("x-request-id").expect("echoed id").to_string();

    assert_eq!(echoed.len(), 36);
    assert_ne!(echoed, "spoofed");
    assert_eq!(origin.seen()[0].header("x-request-id"), Some(echoed.as_str()));

    let second = client.get("/").header("x-request-id").expect("second id").to_string();

    assert_ne!(second, echoed);
    assert_eq!(origin.seen().len(), 1);

    running.stop().expect("stop");

    let trusted = proxy(origin.addr, |config| config.identity.trusted_peers = vec!["127.0.0.1/32".parse().expect("net")]);
    let mut client = Http1::connect(trusted.addr());

    let reply = client.request("GET", "/", &[( "X-Request-Id", "11111111-2222-4333-8444-555555555555" )], b"");

    assert_eq!(reply.header("x-request-id"), Some("11111111-2222-4333-8444-555555555555"));
    assert_eq!(origin.seen()[0].header("x-request-id"), Some("11111111-2222-4333-8444-555555555555"));

    trusted.stop().expect("stop");

}

#[test]
fn forwarded_headers_follow_trust () {

    let origin = Origin::start();
    let running = proxy(origin.addr, |_| {});
    let mut client = Http1::connect(running.addr());

    client.request("GET", "/", &[( "X-Forwarded-For", "1.2.3.4" ), ( "X-Forwarded-Proto", "https" ), ( "Forwarded", "for=9.9.9.9" ), ( "X-Real-Ip", "8.8.8.8" )], b"");

    let seen = origin.seen();

    assert_eq!(seen[0].header("x-forwarded-for"), Some("127.0.0.1"));
    assert_eq!(seen[0].header("x-forwarded-proto"), Some("http"));
    assert_eq!(seen[0].header("forwarded"), None);
    assert_eq!(seen[0].header("x-real-ip"), None);

    running.stop().expect("stop");

    let trusted = proxy(origin.addr, |config| config.identity.trusted_peers = vec!["127.0.0.0/8".parse().expect("net")]);
    let mut client = Http1::connect(trusted.addr());

    client.request("GET", "/", &[( "X-Forwarded-For", "1.2.3.4" )], b"");

    assert_eq!(origin.seen()[0].header("x-forwarded-for"), Some("1.2.3.4, 127.0.0.1"));

    trusted.stop().expect("stop");

}

#[test]
fn weighted_round_robin_distributes_exactly () {

    let a = Origin::start();
    let b = Origin::start();

    let running = proxy(a.addr, |config| {

        config.pools.clear();
        pool(config, "default", &[( a.addr, 1 ), ( b.addr, 3 )]);

    });

    let mut client = Http1::connect(running.addr());

    for _ in 0..400 { assert_eq!(client.get("/").status, 200); }

    assert_eq!(a.seen().len(), 100);
    assert_eq!(b.seen().len(), 300);

    running.stop().expect("stop");

}

#[test]
fn least_conn_alternates_between_idle_backends () {

    let a = Origin::start();
    let b = Origin::start();

    let running = proxy(a.addr, |config| {

        config.pools.clear();
        pool(config, "default", &[( a.addr, 1 ), ( b.addr, 1 )]);
        config.pools.get_mut("default").expect("pool").options.policy = Balance::LeastConn;

    });

    let mut client = Http1::connect(running.addr());

    for _ in 0..100 { assert_eq!(client.get("/").status, 200); }

    assert_eq!(a.seen().len(), 50);
    assert_eq!(b.seen().len(), 50);

    running.stop().expect("stop");

}

#[test]
fn fails_over_and_quarantines_dead_backends () {

    let live = Origin::start();
    let dead = free_port();

    let running = proxy(live.addr, |config| {

        config.pools.clear();
        pool(config, "default", &[( dead, 1 ), ( live.addr, 1 )]);

        let options = &mut config.pools.get_mut("default").expect("pool").options;

        options.max_fails = 1;
        options.attempts = 2;

    });

    let mut client = Http1::connect(running.addr());
    let started = Instant::now();

    for _ in 0..50 { assert_eq!(client.get("/").status, 200); }

    assert!(started.elapsed() < Duration::from_secs(2));
    assert_eq!(live.seen().len(), 50);

    let runtime = running.state().load();
    let backend = &runtime.pools.list[0].backends[0];

    assert!(backend.down_until.load(Ordering::Relaxed) > 0);

    running.stop().expect("stop");

}

#[test]
fn the_last_backend_is_never_quarantined_out_of_service () {

    let origin = Origin::start();
    let running = proxy(origin.addr, |config| {

        let options = &mut config.pools.get_mut("default").expect("pool").options;

        options.max_fails = 1;
        options.cooldown_ms = 60_000;
        options.attempts = 2;
        options.retry_on = vec!["connect".to_string(), "error".to_string(), "5xx".to_string()];

    });

    let mut client = Http1::connect(running.addr());

    assert_eq!(client.get("/status/503").status, 503);
    assert_eq!(client.get("/status/500").status, 500);
    assert_eq!(client.get("/fine").status, 200);
    assert_eq!(origin.seen().len(), 3);

    let runtime = running.state().load();
    let backend = &runtime.pools.list[0].backends[0];

    backend.down_until.store(u64::MAX, Ordering::Relaxed);

    assert_eq!(client.get("/still-served").status, 200);

    backend.probed.store(false, Ordering::Relaxed);

    assert_eq!(client.get("/really-down").status, 503);

    running.stop().expect("stop");

}

#[test]
fn active_probes_mark_dead_backends () {

    let live = Origin::start();
    let dead = free_port();

    let running = proxy(live.addr, |config| {

        config.pools.clear();
        pool(config, "default", &[( dead, 1 ), ( live.addr, 1 )]);
        config.pools.get_mut("default").expect("pool").options.health = Some(HealthConfig { interval_ms: 100, timeout_ms: 100, path: None, status: 200, body: None });

    });

    thread::sleep(Duration::from_millis(600));

    let runtime = running.state().load();

    assert!(!runtime.pools.list[0].backends[0].probed.load(Ordering::Relaxed));
    assert!(runtime.pools.list[0].backends[1].probed.load(Ordering::Relaxed));

    let mut client = Http1::connect(running.addr());

    for _ in 0..20 { assert_eq!(client.get("/work").status, 200); }

    assert_eq!(live.seen().iter().filter(|seen| seen.path == "/work").count(), 20);

    running.stop().expect("stop");

}

#[test]
fn reload_switches_traffic_without_dropping_connections () {

    let a = Origin::start();
    let b = Origin::start();
    let running = proxy(a.addr, |_| {});
    let mut client = Http1::connect(running.addr());

    assert_eq!(client.get("/one").status, 200);
    assert_eq!(a.seen().len(), 1);

    let mut next = Config { listen: running.addr(), ..Config::default() };

    next.set_upstream(b.addr);
    next.runtime.workers = 2;
    next.runtime.pin = false;

    assert_eq!(running.reload(next).expect("reload"), 2);
    assert_eq!(client.get("/two").status, 200);
    assert_eq!(b.seen().len(), 1);
    assert!(a.seen().is_empty());

    let mut moved = Config { listen: free_port(), ..Config::default() };

    moved.set_upstream(b.addr);

    assert!(running.reload(moved).is_err());
    assert_eq!(client.get("/three").status, 200);
    assert_eq!(b.seen().len(), 1);

    running.stop().expect("stop");

}

#[test]
fn caps_in_flight_requests_per_worker () {

    let origin = Origin::start();
    let running = proxy(origin.addr, |config| config.limits.max_in_flight = 2);
    let addr = running.addr();

    let handles: Vec<_> = (0..4).map(|_| thread::spawn(move || Http1::connect(addr).get("/slow/600").status)).collect();
    let statuses: Vec<u16> = handles.into_iter().map(|handle| handle.join().expect("client thread")).collect();

    assert!(statuses.iter().filter(|status| **status == 503).count() >= 2, "{statuses:?}");
    assert!(statuses.contains(&200), "{statuses:?}");
    assert_eq!(Http1::connect(addr).get("/").status, 200);

    running.stop().expect("stop");

}

#[test]
fn cuts_upstream_bodies_that_stall () {

    let origin = Origin::start();
    let running = proxy(origin.addr, |config| config.limits.timeout_ms = 300);
    let mut client = Http1::connect(running.addr());
    let started = Instant::now();

    client.send(b"GET /chunked/3/16/1500 HTTP/1.1\r\nHost: test.local\r\n\r\n");

    let reply = client.try_reply();
    let elapsed = started.elapsed();

    assert!(reply.is_none(), "stalled body was delivered whole");
    assert!(elapsed < Duration::from_millis(1_200), "stall lasted {elapsed:?}");

    running.stop().expect("stop");

}

#[test]
fn rate_limits_count_per_actor_in_a_ten_second_window () {

    let origin = Origin::start();

    let running = proxy(origin.addr, |config| {

        config.limits.rate_limit_10s = 3;
        config.identity.trusted_peers = vec!["127.0.0.1/32".parse().expect("net")];
        config.identity.actor_header = Some("x-actor".to_string());

    });

    let mut client = Http1::connect(running.addr());

    for _ in 0..3 { assert_eq!(client.get("/").status, 200); }

    assert_eq!(client.get("/").status, 429);
    assert_eq!(Http1::connect(running.addr()).get("/").status, 429);

    for _ in 0..3 { assert_eq!(client.request("GET", "/", &[( "x-actor", "tenant-b" )], b"").status, 200); }

    assert_eq!(client.request("GET", "/", &[( "x-actor", "tenant-b" )], b"").status, 429);
    assert_eq!(client.request("GET", "/", &[( "x-actor", "tenant-c" )], b"").status, 200);
    assert_eq!(origin.seen().len(), 7);

    running.stop().expect("stop");

}

#[test]
fn untrusted_actor_headers_share_the_peer_budget_and_routes_override () {

    let origin = Origin::start();

    let running = proxy(origin.addr, |config| {

        config.identity.actor_header = Some("x-actor".to_string());
        config.routes.push(Route { name: "all".to_string(), path: "/".to_string(), upstream: "default".to_string(), ..Route::default() });
        config.routes.push(Route { name: "tight".to_string(), path: "/tight".to_string(), upstream: "default".to_string(), rate_limit_10s: Some(2), ..Route::default() });

    });

    let mut client = Http1::connect(running.addr());

    for _ in 0..5 { assert_eq!(client.request("GET", "/open", &[( "x-actor", "a" )], b"").status, 200); }

    assert_eq!(client.request("GET", "/tight", &[( "x-actor", "a" )], b"").status, 200);
    assert_eq!(client.request("GET", "/tight", &[( "x-actor", "b" )], b"").status, 200);
    assert_eq!(client.request("GET", "/tight", &[( "x-actor", "c" )], b"").status, 429);
    assert_eq!(client.get("/open").status, 200);

    running.stop().expect("stop");

}

#[test]
fn retries_follow_the_pool_policy_and_replayable_bodies () {

    let flaky = Origin::start();
    let steady = Origin::start();

    let running = proxy(flaky.addr, |config| {

        let pool = config.pools.get_mut("default").expect("pool");

        pool.backends.push(BackendConfig { address: steady.addr.into(), ..BackendConfig::default() });
        pool.options.attempts = 2;
        pool.options.retry_on = vec!["connect".to_string(), "503".to_string()];
        pool.options.retry_non_idempotent = true;
        pool.options.max_fails = 10;

        config.routes.push(Route { name: "all".to_string(), path: "/".to_string(), upstream: "default".to_string(), ..Route::default() });
        config.routes.push(Route { name: "buffered".to_string(), path: "/buffered".to_string(), upstream: "default".to_string(), buffer_request: Some(true), ..Route::default() });

    });

    let mut client = Http1::connect(running.addr());

    flaky.flaky(1);
    assert_eq!(client.get("/retry-get").status, 200);
    assert_eq!(flaky.seen().len() + steady.seen().len(), 2);

    flaky.flaky(1);
    steady.flaky(0);

    let unbuffered = client.request("POST", "/echo", &[], b"payload");

    assert!(unbuffered.status == 503 || unbuffered.status == 200, "{}", unbuffered.status);

    flaky.flaky(2);
    steady.flaky(0);

    let buffered = client.request("POST", "/buffered/echo", &[], b"payload");

    assert_eq!(buffered.status, 200);

    running.stop().expect("stop");

}

#[test]
fn buffered_requests_reach_the_origin_whole () {

    let origin = Origin::start();
    let running = proxy(origin.addr, |config| config.limits.buffer_requests = true);
    let mut client = Http1::connect(running.addr());

    client.send(b"POST /echo HTTP/1.1\r\nHost: test.local\r\nContent-Length: 10\r\n\r\nhello");
    thread::sleep(Duration::from_millis(150));
    client.send(b"world");

    let reply = client.reply();

    assert_eq!(reply.status, 200);
    assert_eq!(reply.text(), "helloworld");
    assert_eq!(origin.seen()[0].body, b"helloworld");

    running.stop().expect("stop");

}

#[test]
fn health_checks_can_require_a_response_body () {

    let origin = Origin::start();
    let mut seed = None;

    let running = proxy(origin.addr, |config| {

        let check = |path: &str, body: &str| Some(HealthConfig { interval_ms: 100, timeout_ms: 500, path: Some(path.to_string()), status: 200, body: Some(body.to_string()) });

        config.pools.clear();

        for ( name, health ) in [( "default", check("/healthz", "ok") ), ( "pattern", check("/etagged", "~^fresh b") ), ( "strict", check("/healthz", "ready") ), ( "literal", check("/etagged", "fresh.body") )] {

            pool(config, name, &[( origin.addr, 1 )]);
            config.pools.get_mut(name).expect("pool").options.health = health;

        }

        seed = Some(config.clone());

    });

    thread::sleep(Duration::from_millis(700));

    let runtime = running.state().load();
    let healthy = |name: &str| runtime.pools.list.iter().find(|pool| &*pool.name == name).expect("pool").backends[0].probed.load(Ordering::Relaxed);

    assert!(healthy("default"), "the body contains the expected text");
    assert!(healthy("pattern"), "the body matches the pattern");
    assert!(!healthy("strict"), "a 200 with the wrong body is unhealthy");
    assert!(!healthy("literal"), "plain text is matched literally, not as a pattern");

    let mut broken = seed.expect("seed");

    broken.pools.get_mut("pattern").expect("pool").options.health.as_mut().expect("health").body = Some("~(".to_string());

    assert!(running.reload(broken).is_err(), "an invalid pattern is rejected");

    running.stop().expect("stop");

}

#[test]
fn slow_answers_count_as_failures_and_eject_the_backend () {

    let ( a, b ) = ( Origin::start(), Origin::start() );
    let running = proxy(a.addr, |config| {

        config.pools.clear();
        pool(config, "default", &[( a.addr, 1 ), ( b.addr, 1 )]);

        let options = &mut config.pools.get_mut("default").expect("pool").options;

        options.slow_ms = 100;
        options.max_fails = 1;

    });
    let mut client = Http1::connect(running.addr());

    assert_eq!(client.get("/slow/300").status, 200, "the slow answer itself is still delivered");

    a.seen();
    b.seen();

    for _ in 0..8 { assert_eq!(client.get("/fast").status, 200); }

    let ( on_a, on_b ) = ( a.seen().len(), b.seen().len() );

    assert!(( on_a == 0 && on_b == 8 ) || ( on_a == 8 && on_b == 0 ), "one backend sits out after its slow answer: {on_a} / {on_b}");

    running.stop().expect("stop");

}

