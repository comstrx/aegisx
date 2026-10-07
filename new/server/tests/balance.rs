mod support;

use std::collections::HashSet;
use std::net::SocketAddr;

use aegisx::config::{BackendConfig, Balance, Config, StickyConfig};
use support::{Http1, Origin, proxy};

fn pool ( config: &mut Config, backends: &[SocketAddr], policy: Balance, key: Option<&str>, sticky: Option<StickyConfig> ) {

    let entry = config.pools.entry("default".to_string()).or_default();

    entry.backends = backends.iter().map(|address| BackendConfig { address: (*address).into(), ..BackendConfig::default() }).collect();
    entry.options.policy = policy;
    entry.options.hash_key = key.map(str::to_owned);
    entry.options.sticky = sticky;

}

#[test]
fn ip_hash_pins_a_client_to_one_backend () {

    let first = Origin::start();
    let second = Origin::start();
    let running = proxy(first.addr, |config| pool(config, &[first.addr, second.addr], Balance::IpHash, None, None));
    let mut client = Http1::connect(running.addr());

    for index in 0..40 { assert_eq!(client.get(&format!("/{index}")).status, 200); }

    let ( a, b ) = ( first.seen().len(), second.seen().len() );

    assert_eq!(a + b, 40);
    assert!(a == 40 || b == 40, "ip_hash spread requests: {a} / {b}");

    running.stop().expect("stop");

}

#[test]
fn header_hash_spreads_keys_and_keeps_each_key_stable () {

    let first = Origin::start();
    let second = Origin::start();
    let running = proxy(first.addr, |config| pool(config, &[first.addr, second.addr], Balance::Hash, Some("header:x-user"), None));
    let mut client = Http1::connect(running.addr());

    for user in 0..32 {

        for _ in 0..3 { assert_eq!(client.request("GET", "/", &[( "X-User", &format!("user-{user}") )], b"").status, 200); }

    }

    let ( a, b ) = ( first.seen(), second.seen() );

    assert_eq!(a.len() + b.len(), 96);
    assert!(!a.is_empty() && !b.is_empty(), "hash sent everything to one backend: {} / {}", a.len(), b.len());

    let users_a: HashSet<String> = a.iter().filter_map(|seen| seen.header("x-user").map(str::to_owned)).collect();
    let users_b: HashSet<String> = b.iter().filter_map(|seen| seen.header("x-user").map(str::to_owned)).collect();

    assert!(users_a.is_disjoint(&users_b), "a user reached both backends");

    running.stop().expect("stop");

}

#[test]
fn sticky_cookies_pin_and_recover () {

    let first = Origin::start();
    let second = Origin::start();
    let running = proxy(first.addr, |config| pool(config, &[first.addr, second.addr], Balance::RoundRobin, None, Some(StickyConfig { cookie: "srv".to_string(), ttl_ms: 60_000, ..StickyConfig::default() })));
    let mut client = Http1::connect(running.addr());

    let opening = client.get("/first");
    let cookie = opening.header("set-cookie").expect("sticky cookie").to_string();

    assert!(cookie.starts_with("srv="), "{cookie}");
    assert!(cookie.contains("Max-Age=60") && cookie.contains("HttpOnly") && cookie.contains("SameSite=Lax"), "{cookie}");

    let pair = cookie.split(';').next().expect("pair").to_string();

    for index in 0..20 {

        let reply = client.request("GET", &format!("/again/{index}"), &[( "Cookie", &pair )], b"");

        assert_eq!(reply.status, 200);
        assert_eq!(reply.header("set-cookie"), None, "cookie re-issued while pinned");

    }

    let ( a, b ) = ( first.seen().len(), second.seen().len() );

    assert!(a == 21 || b == 21, "sticky requests spread: {a} / {b}");

    let stale = client.request("GET", "/stale", &[( "Cookie", "srv=deadbeefdeadbeef" )], b"");

    assert_eq!(stale.status, 200);
    assert!(stale.header("set-cookie").is_some_and(|value| value.starts_with("srv=")), "unknown cookie was not replaced");

    running.stop().expect("stop");

}

#[test]
fn balancer_accepts_policy_strings_and_sticky_tables_in_lua () {

    let config = Config::parse(r#"
        add_upstream("api", "127.0.0.1:3001")
        add_upstream("api", "127.0.0.1:3002")
        set_balancer("api", "ip_hash")
        set_sticky("api", { cookie = "sid", ttl_ms = 1000 })
        set_balancer("web", { policy = "hash", hash_key = "cookie:session" })
        add_upstream("web", "127.0.0.1:3003")
        set_default_upstream("api")
    "#, "balance.lua", std::path::Path::new("/tmp")).expect("config");

    assert_eq!(config.pools["api"].options.policy, Balance::IpHash);
    assert_eq!(config.pools["api"].options.sticky.as_ref().map(|sticky| sticky.cookie.as_str()), Some("sid"));
    assert_eq!(config.pools["web"].options.policy, Balance::Hash);
    assert_eq!(config.pools["web"].options.hash_key.as_deref(), Some("cookie:session"));

    let bad = Config::parse(r#"
        add_upstream("api", "127.0.0.1:3001")
        set_balancer("api", { policy = "hash", hash_key = "nonsense" })
    "#, "balance.lua", std::path::Path::new("/tmp")).expect_err("bad hash key").to_string();

    assert!(bad.contains("hash_key"), "{bad}");

}

#[test]
fn random_picks_the_less_loaded_of_two_backends () {

    let first = Origin::start();
    let second = Origin::start();
    let third = Origin::start();
    let running = proxy(first.addr, |config| pool(config, &[first.addr, second.addr, third.addr], Balance::Random, None, None));
    let mut client = Http1::connect(running.addr());

    for index in 0..90 { assert_eq!(client.get(&format!("/{index}")).status, 200); }

    let seen = [first.seen().len(), second.seen().len(), third.seen().len()];

    assert_eq!(seen.iter().sum::<usize>(), 90);
    assert!(seen.iter().all(|count| *count >= 10), "random left a backend idle: {seen:?}");

    let parsed = Config::parse(r#"add_upstream("default", "127.0.0.1:3000") set_balancer("default", "random")"#, "balance.lua", std::path::Path::new("/tmp")).expect("parse");

    assert_eq!(parsed.pools["default"].options.policy, Balance::Random);

    running.stop().expect("stop");

}

#[test]
fn slow_start_ramps_a_recovered_backend_and_the_ejection_cap_keeps_one_alive () {

    let first = Origin::start();
    let second = Origin::start();
    let running = proxy(first.addr, |config| {

        pool(config, &[first.addr, second.addr], Balance::RoundRobin, None, None);
        config.pools.get_mut("default").expect("pool").options.slow_start_ms = 60_000;

    });

    let runtime = running.state().load();

    runtime.pools.list[0].backends[0].revived.store(runtime.pools.now_ms().max(1), std::sync::atomic::Ordering::Relaxed);

    let mut client = Http1::connect(running.addr());

    for index in 0..60 { assert_eq!(client.get(&format!("/{index}")).status, 200); }

    let ( ramping, steady ) = ( first.seen().len(), second.seen().len() );

    assert!(ramping <= 6 && steady >= 54, "slow start sent {ramping} / {steady}");

    running.stop().expect("stop");

    let ( dead, gone ) = ( support::free_port(), support::free_port() );
    let running = proxy(dead, |config| {

        pool(config, &[dead, gone], Balance::RoundRobin, None, None);

        let options = &mut config.pools.get_mut("default").expect("pool").options;

        options.max_fails = 1;
        options.max_ejected = 50;
        options.attempts = 2;

    });

    for _ in 0..4 { assert_eq!(Http1::connect(running.addr()).get("/").status, 502); }

    let runtime = running.state().load();
    let quarantined = runtime.pools.list[0].backends.iter().filter(|backend| backend.down_until.load(std::sync::atomic::Ordering::Relaxed) > 0).count();

    assert_eq!(quarantined, 1);

    running.stop().expect("stop");

}
