mod support;

use std::collections::BTreeMap;
use std::net::SocketAddr;
use std::thread;
use std::time::{Duration, Instant};

use aegisx::config::{Config, ControlConfig, Route};
use serde_json::Value;
use support::{Http1, Origin, Reply, free_port, proxy};

const ADMIN: &str = "test-admin-token-0123456789abcdef0123456789abcdef";

fn cached ( config: &mut Config, valid_ms: u64, stale_ms: u64 ) {

    config.cache.enabled = true;
    config.cache.valid_ms = BTreeMap::from([( "200".to_string(), valid_ms )]);
    config.cache.stale_ms = stale_ms;
    config.response_headers.insert("content-type".to_string(), "text/plain".to_string());

}

fn admin ( addr: SocketAddr, method: &str, path: &str, body: &[u8] ) -> Reply {

    let host = addr.to_string();
    let auth = format!("Bearer {ADMIN}");
    let mut headers = vec![( "Host", host.as_str() ), ( "Authorization", auth.as_str() )];

    if !body.is_empty() { headers.push(( "Content-Type", "application/json" )); }

    Http1::connect(addr).request(method, path, &headers, body)

}

#[test]
fn second_request_is_served_from_memory () {

    let origin = Origin::start();
    let running = proxy(origin.addr, |config| cached(config, 60_000, 0));
    let mut client = Http1::connect(running.addr());

    let first = client.get("/asset?v=1");

    assert_eq!(first.status, 200);
    assert_eq!(first.header("x-cache"), None);

    let second = client.get("/asset?v=1");

    assert_eq!(second.status, 200);
    assert_eq!(second.header("x-cache"), Some("HIT"));
    assert_eq!(second.header("age"), Some("0"));
    assert_eq!(second.text(), "ok");

    let head = client.request("HEAD", "/asset?v=1", &[], b"");

    assert_eq!(head.status, 200);
    assert_eq!(head.header("x-cache"), Some("HIT"));
    assert_eq!(head.header("content-length"), Some("2"));

    let other = client.get("/asset?v=2");

    assert_eq!(other.header("x-cache"), None);

    let authorized = client.request("GET", "/asset?v=1", &[( "Authorization", "Bearer x" )], b"");

    assert_eq!(authorized.header("x-cache"), None);

    let posted = client.request("POST", "/asset?v=1", &[], b"data");

    assert_eq!(posted.header("x-cache"), None);

    assert_eq!(origin.seen().len(), 4);

    running.stop().expect("stop");

}

#[test]
fn upstream_cache_control_decides_storage () {

    let origin = Origin::start();
    let running = proxy(origin.addr, |config| {

        cached(config, 0, 0);
        config.routes.push(Route { name: "private".to_string(), path: "/private".to_string(), upstream: "default".to_string(), response_headers: BTreeMap::from([( "cache-control".to_string(), "private, max-age=60".to_string() )]), ..Route::default() });
        config.routes.push(Route { name: "public".to_string(), path: "/public".to_string(), upstream: "default".to_string(), response_headers: BTreeMap::from([( "cache-control".to_string(), "public, s-maxage=60".to_string() )]), ..Route::default() });
        config.routes.push(Route { name: "cookie".to_string(), path: "/cookie".to_string(), upstream: "default".to_string(), response_headers: BTreeMap::from([( "cache-control".to_string(), "max-age=60".to_string() ), ( "set-cookie".to_string(), "a=b".to_string() )]), ..Route::default() });
        config.routes.push(Route { name: "plain".to_string(), path: "/".to_string(), upstream: "default".to_string(), ..Route::default() });

    });
    let mut client = Http1::connect(running.addr());

    for _ in 0..2 { assert_eq!(client.get("/private").header("x-cache"), None); }
    for _ in 0..2 { assert_eq!(client.get("/cookie").header("x-cache"), None); }
    for _ in 0..2 { assert_eq!(client.get("/plain").header("x-cache"), None); }

    assert_eq!(client.get("/public").header("x-cache"), None);
    assert_eq!(client.get("/public").header("x-cache"), Some("HIT"));
    assert_eq!(origin.seen().len(), 7);

    running.stop().expect("stop");

}

#[test]
fn stale_entries_are_served_while_one_request_refreshes () {

    let origin = Origin::start();
    let running = proxy(origin.addr, |config| cached(config, 200, 10_000));
    let addr = running.addr();

    assert_eq!(Http1::connect(addr).get("/slow/400").status, 200);

    thread::sleep(Duration::from_millis(350));

    let refresher = thread::spawn(move || Http1::connect(addr).get("/slow/400"));

    thread::sleep(Duration::from_millis(100));

    let started = Instant::now();
    let stale = Http1::connect(addr).get("/slow/400");

    assert_eq!(stale.header("x-cache"), Some("STALE"));
    assert!(started.elapsed() < Duration::from_millis(250), "stale answer waited {:?}", started.elapsed());

    let refreshed = refresher.join().expect("refresher");

    assert_eq!(refreshed.header("x-cache"), None);
    assert_eq!(Http1::connect(addr).get("/slow/400").header("x-cache"), Some("HIT"));

    running.stop().expect("stop");

}

#[test]
fn control_api_reports_and_purges_the_cache () {

    let origin = Origin::start();
    let running = proxy(origin.addr, |config| {

        cached(config, 60_000, 0);
        config.control = ControlConfig { enabled: true, listen: free_port(), token_env: "AEGISX_TEST_ADMIN_TOKEN".to_string(), ..ControlConfig::default() };

    });
    let mut client = Http1::connect(running.addr());
    let control = running.control_addr().expect("control");

    assert_eq!(client.get("/one").status, 200);
    assert_eq!(client.get("/one").header("x-cache"), Some("HIT"));

    let stats: Value = serde_json::from_slice(&admin(control, "GET", "/api/v1/cache", b"").body).expect("json");

    assert_eq!(stats["entries"], 1);
    assert_eq!(stats["hits"], 1);
    assert_eq!(stats["misses"], 1);

    let purged = admin(control, "DELETE", "/api/v1/cache", br#"{"key":"test.local /one"}"#);

    assert_eq!(purged.status, 200);

    assert_eq!(client.get("/one").header("x-cache"), None);
    assert_eq!(client.get("/one").header("x-cache"), Some("HIT"));

    let cleared = admin(control, "DELETE", "/api/v1/cache", br#"{"all":true}"#);

    assert_eq!(cleared.status, 200);
    assert_eq!(client.get("/one").header("x-cache"), None);

    running.stop().expect("stop");

}

#[test]
fn stale_entries_are_revalidated_with_conditional_requests () {

    let origin = Origin::start();
    let running = proxy(origin.addr, |config| { config.cache.enabled = true; config.cache.lock = true; config.cache.stale_ms = 60_000; });
    let mut client = Http1::connect(running.addr());

    let first = client.get("/etagged");

    assert_eq!(first.status, 200);
    assert_eq!(first.header("x-cache"), None);
    assert_eq!(first.text(), "fresh body");

    let hit = client.get("/etagged");

    assert_eq!(hit.header("x-cache"), Some("HIT"));

    std::thread::sleep(std::time::Duration::from_millis(1_200));

    let revalidated = client.get("/etagged");

    assert_eq!(revalidated.status, 200);
    assert_eq!(revalidated.header("x-cache"), Some("REVALIDATED"));
    assert_eq!(revalidated.text(), "fresh body");
    assert_eq!(revalidated.header("etag"), Some("\"v1\""));

    let seen = origin.seen();

    assert_eq!(seen.len(), 2, "{seen:?}");
    assert_eq!(seen[1].header("if-none-match"), Some("\"v1\""));

    let again = client.get("/etagged");

    assert_eq!(again.header("x-cache"), Some("HIT"));
    assert_eq!(origin.seen().len(), 0);

    running.stop().expect("stop");

}


#[test]
fn key_headers_split_entries_and_stale_entries_cover_upstream_failures () {

    let origin = Origin::start();
    let running = proxy(origin.addr, |config| {

        cached(config, 300, 30_000);
        config.cache.key_headers = vec!["x-tenant".to_string()];
        config.cache.stale_if_error = true;

    });
    let mut client = Http1::connect(running.addr());

    assert_eq!(client.request("GET", "/public", &[( "x-tenant", "a" )], b"").header("x-cache"), None);
    assert_eq!(client.request("GET", "/public", &[( "x-tenant", "b" )], b"").header("x-cache"), None);
    assert_eq!(client.request("GET", "/public", &[( "x-tenant", "a" )], b"").header("x-cache"), Some("HIT"));
    assert_eq!(origin.seen().len(), 2);

    std::thread::sleep(std::time::Duration::from_millis(450));
    origin.flaky(1);

    let rescued = client.request("GET", "/public", &[( "x-tenant", "a" )], b"");

    assert_eq!(rescued.status, 200);
    assert_eq!(rescued.header("x-cache"), Some("STALE"));
    assert_eq!(origin.seen().len(), 1);

    let later = client.request("GET", "/public", &[( "x-tenant", "a" )], b"");

    assert_eq!(later.header("x-cache"), Some("STALE"));
    assert_eq!(origin.seen().len(), 0);

    running.stop().expect("stop");

}

#[test]
fn vary_splits_entries_by_the_named_request_headers () {

    let origin = Origin::start();
    let running = proxy(origin.addr, |config| {

        cached(config, 60_000, 0);
        config.routes.push(Route { name: "varied".to_string(), path: "/varied".to_string(), upstream: "default".to_string(), response_headers: BTreeMap::from([( "vary".to_string(), "X-Flavor".to_string() )]), ..Route::default() });
        config.routes.push(Route { name: "opaque".to_string(), path: "/opaque".to_string(), upstream: "default".to_string(), response_headers: BTreeMap::from([( "vary".to_string(), "*".to_string() )]), ..Route::default() });
        config.routes.push(Route { name: "plain".to_string(), path: "/".to_string(), upstream: "default".to_string(), ..Route::default() });

    });
    let mut client = Http1::connect(running.addr());
    let mut taste = |flavor: &str| client.request("GET", "/varied", &[( "X-Flavor", flavor )], b"").header("x-cache").map(str::to_owned);

    assert_eq!([taste("sweet"), taste("sweet"), taste("sweet")], [None, None, Some("HIT".to_string())]);
    assert_eq!([taste("sour"), taste("sour")], [None, Some("HIT".to_string())]);
    assert_eq!(taste("sweet"), Some("HIT".to_string()));
    assert_eq!(origin.seen().len(), 3);

    for _ in 0..3 { assert_eq!(client.get("/opaque").header("x-cache"), None); }

    assert_eq!(origin.seen().len(), 3);

    running.stop().expect("stop");

}

#[test]
fn ignored_headers_force_the_configured_lifetime () {

    let origin = Origin::start();
    let running = proxy(origin.addr, |config| {

        cached(config, 60_000, 0);
        config.cache.ignore_headers = vec!["Cache-Control".to_string(), "Set-Cookie".to_string(), "vary".to_string()];
        config.routes.push(Route { name: "private".to_string(), path: "/private".to_string(), upstream: "default".to_string(), response_headers: BTreeMap::from([( "cache-control".to_string(), "private, no-store".to_string() ), ( "vary".to_string(), "*".to_string() )]), ..Route::default() });
        config.routes.push(Route { name: "plain".to_string(), path: "/".to_string(), upstream: "default".to_string(), ..Route::default() });

    });
    let mut client = Http1::connect(running.addr());

    assert_eq!(client.get("/private").header("x-cache"), None);
    assert_eq!(client.get("/private").header("x-cache"), Some("HIT"));
    assert!(client.get("/cookie").header("set-cookie").is_some());

    let stored = client.get("/cookie");

    assert_eq!(stored.header("x-cache"), Some("HIT"));
    assert_eq!(stored.header("set-cookie"), None);
    assert_eq!(origin.seen().len(), 2);

    running.stop().expect("stop");

    let bad = aegisx::config::Config::parse(r#"set_upstream("127.0.0.1:3000") set_cache { enabled = true, ignore_headers = { "etag" } }"#, "cache.lua", std::path::Path::new("/tmp"));

    assert!(bad.is_err());

}

#[test]
fn concurrent_misses_for_one_key_reach_the_origin_once () {

    let origin = Origin::start();
    let running = proxy(origin.addr, |config| cached(config, 60_000, 0));
    let addr = running.addr();

    let waiters: Vec<_> = (0..6).map(|_| thread::spawn(move || {

        let reply = Http1::connect(addr).get("/slow/300");

        ( reply.status, reply.header("x-cache").map(str::to_owned) )

    })).collect();

    let replies: Vec<_> = waiters.into_iter().map(|waiter| waiter.join().expect("waiter")).collect();

    assert!(replies.iter().all(|( status, _ )| *status == 200), "{replies:?}");
    assert_eq!(replies.iter().filter(|( _, cache )| cache.as_deref() == Some("HIT")).count(), 5, "{replies:?}");
    assert_eq!(origin.seen().len(), 1);

    running.stop().expect("stop");

}

#[test]
fn uncacheable_answers_stop_followers_from_queueing () {

    let origin = Origin::start();
    let running = proxy(origin.addr, |config| {

        cached(config, 60_000, 0);
        config.routes.push(Route { name: "private".to_string(), path: "/slow".to_string(), upstream: "default".to_string(), response_headers: BTreeMap::from([( "cache-control".to_string(), "private".to_string() )]), ..Route::default() });
        config.routes.push(Route { name: "plain".to_string(), path: "/".to_string(), upstream: "default".to_string(), ..Route::default() });

    });
    let addr = running.addr();

    assert_eq!(Http1::connect(addr).get("/slow/50").status, 200);

    let started = Instant::now();
    let pair: Vec<_> = (0..4).map(|_| thread::spawn(move || Http1::connect(addr).get("/slow/50").status)).collect();

    for waiter in pair { assert_eq!(waiter.join().expect("waiter"), 200); }

    assert!(started.elapsed() < Duration::from_millis(400), "followers queued for {:?}", started.elapsed());
    assert_eq!(origin.seen().len(), 5);

    running.stop().expect("stop");

}

#[test]
fn the_disk_tier_survives_a_restart_and_purges_with_the_key () {

    let origin = Origin::start();
    let dir = std::env::temp_dir().join(format!("aegisx-shelf-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let control = free_port();

    let boot = |control: SocketAddr| proxy(origin.addr, |config| {

        cached(config, 60_000, 0);
        config.cache.path = Some(dir.clone());
        config.control = ControlConfig { enabled: true, listen: control, token_env: "AEGISX_TEST_ADMIN_TOKEN".to_string(), ..ControlConfig::default() };

    });

    let running = boot(control);
    let mut client = Http1::connect(running.addr());

    assert_eq!(client.get("/kept").header("x-cache"), None);
    assert_eq!(client.get("/kept").header("x-cache"), Some("HIT"));

    drop(client);
    thread::sleep(Duration::from_millis(300));
    running.stop().expect("stop");

    assert!(std::fs::read_dir(&dir).expect("cache dir").flatten().any(|shard| shard.path().is_dir()));

    let control = free_port();
    let running = boot(control);
    let mut client = Http1::connect(running.addr());

    assert_eq!(client.get("/kept").header("x-cache"), Some("HIT"));
    assert_eq!(origin.seen().len(), 1);

    assert_eq!(admin(control, "DELETE", "/api/v1/cache", br#"{"key":"test.local /kept"}"#).status, 200);
    assert_eq!(client.get("/kept").header("x-cache"), None);
    assert_eq!(origin.seen().len(), 1);

    running.stop().expect("stop");

    let _ = std::fs::remove_dir_all(&dir);

}

fn state ( client: &mut Http1, path: &str ) -> Option<String> {

    client.get(path).header("x-cache").map(str::to_owned)

}

#[test]
fn bans_purge_by_prefix_host_and_tag () {

    let origin = Origin::start();
    let running = proxy(origin.addr, |config| {

        cached(config, 60_000, 0);
        config.control = ControlConfig { enabled: true, listen: free_port(), token_env: "AEGISX_TEST_ADMIN_TOKEN".to_string(), ..ControlConfig::default() };
        config.routes.push(Route { name: "blog".to_string(), path: "/blog".to_string(), upstream: "default".to_string(), response_headers: BTreeMap::from([( "cache-tag".to_string(), "blog, post-7".to_string() )]), ..Route::default() });
        config.routes.push(Route { name: "shop".to_string(), path: "/shop".to_string(), upstream: "default".to_string(), response_headers: BTreeMap::from([( "surrogate-key".to_string(), "shop catalog".to_string() )]), ..Route::default() });
        config.routes.push(Route { name: "rest".to_string(), path: "/".to_string(), upstream: "default".to_string(), ..Route::default() });

    });
    let mut client = Http1::connect(running.addr());
    let control = running.control_addr().expect("control");
    let ban = |body: &[u8]| { let reply = admin(control, "DELETE", "/api/v1/cache", body); thread::sleep(Duration::from_millis(5)); reply.status };

    for path in ["/blog/a", "/blog/b", "/shop/x", "/other"] {

        assert_eq!(state(&mut client, path), None);
        assert_eq!(state(&mut client, path).as_deref(), Some("HIT"));

    }

    assert_eq!(ban(br#"{"prefix":"/blog/"}"#), 200);
    assert_eq!([state(&mut client, "/blog/a"), state(&mut client, "/blog/b")], [None, None], "the prefix is gone");
    assert_eq!(state(&mut client, "/blog/a").as_deref(), Some("HIT"), "and is stored again afterwards");
    assert_eq!([state(&mut client, "/shop/x").as_deref(), state(&mut client, "/other").as_deref()], [Some("HIT"), Some("HIT")], "other paths stay");

    assert_eq!(ban(br#"{"tag":"catalog"}"#), 200);
    assert_eq!(state(&mut client, "/shop/x"), None, "surrogate keys are tags");
    assert_eq!(state(&mut client, "/blog/a").as_deref(), Some("HIT"));

    assert_eq!(ban(br#"{"tag":"post-7"}"#), 200);
    assert_eq!(state(&mut client, "/blog/a"), None, "cache-tag lists are tags");
    assert_eq!(state(&mut client, "/other").as_deref(), Some("HIT"));

    assert_eq!(ban(br#"{"host":"elsewhere.local","prefix":"/"}"#), 200);
    assert_eq!(state(&mut client, "/other").as_deref(), Some("HIT"), "another host is another site");

    assert_eq!(ban(br#"{"host":"test.local","prefix":"/other"}"#), 200);
    assert_eq!(state(&mut client, "/other"), None);

    let stats: Value = serde_json::from_slice(&admin(control, "GET", "/api/v1/cache", b"").body).expect("json");

    assert_eq!(stats["bans"], 5);
    assert_eq!(ban(br#"{"prefix":"blog"}"#), 400, "a prefix is a path");
    assert_eq!(ban(br#"{"tag":"two words"}"#), 400);
    assert_eq!(ban(br#"{}"#), 400);
    assert_eq!(ban(br#"{"all":true}"#), 200);
    assert_eq!(serde_json::from_slice::<Value>(&admin(control, "GET", "/api/v1/cache", b"").body).expect("json")["bans"], 0, "a full purge forgets the bans");

    running.stop().expect("stop");

}


#[test]
fn ranges_are_answered_from_cached_objects_and_never_fill_the_cache () {

    let origin = Origin::start();
    let running = proxy(origin.addr, |config| cached(config, 60_000, 0));
    let mut client = Http1::connect(running.addr());
    let ranged = |client: &mut Http1, range: &str, extra: Option<( &str, &str )>| client.request("GET", "/blob", &[( "Range", range )].into_iter().chain(extra).collect::<Vec<_>>(), b"");

    assert_eq!(ranged(&mut client, "bytes=0-0", None).header("x-cache"), None, "a ranged miss goes to the upstream");
    assert_eq!(state(&mut client, "/blob"), None, "and stores nothing");
    assert_eq!(state(&mut client, "/blob").as_deref(), Some("HIT"));

    let first = ranged(&mut client, "bytes=0-0", None);

    assert_eq!(( first.status, first.text().as_str(), first.header("content-range"), first.header("x-cache") ), ( 206, "o", Some("bytes 0-0/2"), Some("HIT") ));
    assert_eq!(ranged(&mut client, "bytes=-1", None).text(), "k");
    assert_eq!(ranged(&mut client, "bytes=1-", None).header("content-length"), Some("1"));

    let beyond = ranged(&mut client, "bytes=5-9", None);

    assert_eq!(( beyond.status, beyond.header("content-range") ), ( 416, Some("bytes */2") ));
    assert_eq!(ranged(&mut client, "bytes=0-0,1-1", None).status, 200, "several ranges get the whole object");
    assert_eq!(ranged(&mut client, "bytes=0-0", Some(( "If-Range", "\"unknown\"" ))).status, 200, "an If-Range that cannot be confirmed gets the whole object");
    assert_eq!(origin.seen().len(), 2);

    running.stop().expect("stop");

}

#[test]
fn x_accel_expires_decides_the_lifetime_and_a_variable_bypasses_the_cache () {

    use aegisx::http::variable::{Kind, Recipe};

    let origin = Origin::start();
    let running = proxy(origin.addr, |config| {

        cached(config, 60_000, 0);
        config.variables.insert("skip".to_string(), Recipe { kind: Kind::Map, from: "header:x-no-cache".to_string(), values: BTreeMap::from([( "1".to_string(), "1".to_string() ), ( "0".to_string(), "0".to_string() )]), ..Recipe::default() });
        config.routes.push(Route { name: "never".to_string(), path: "/never".to_string(), upstream: "default".to_string(), response_headers: BTreeMap::from([( "x-accel-expires".to_string(), "0".to_string() )]), ..Route::default() });
        config.routes.push(Route { name: "choosy".to_string(), path: "/choosy".to_string(), upstream: "default".to_string(), cache_bypass: vec!["skip".to_string()], ..Route::default() });
        config.routes.push(Route { name: "rest".to_string(), path: "/".to_string(), upstream: "default".to_string(), ..Route::default() });

    });
    let mut client = Http1::connect(running.addr());
    let with = |client: &mut Http1, value: &str| client.request("GET", "/choosy", &[( "X-No-Cache", value )], b"").header("x-cache").map(str::to_owned);

    assert_eq!([state(&mut client, "/never"), state(&mut client, "/never")], [None, None], "X-Accel-Expires: 0 wins over the configured lifetime");
    assert_eq!([state(&mut client, "/choosy"), state(&mut client, "/choosy")], [None, Some("HIT".to_string())]);

    origin.seen();

    assert_eq!(with(&mut client, "1"), None, "a set variable skips the cache");
    assert_eq!(origin.seen().len(), 1);
    assert_eq!(with(&mut client, "0").as_deref(), Some("HIT"), "0 counts as unset");
    assert_eq!(state(&mut client, "/choosy").as_deref(), Some("HIT"), "the bypass left the stored object alone");
    assert!(Config::parse(r#"set_upstream("127.0.0.1:3000") add_route { name = "r", path = "/", cache_bypass = { "missing" } }"#, "cache.lua", std::path::Path::new("/tmp")).is_err());

    running.stop().expect("stop");

}
