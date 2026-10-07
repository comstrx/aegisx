mod support;

use std::path::Path;

use aegisx::config::{Config, RewriteRule, Route};
use support::{Http1, Origin, proxy};

fn rule ( from: &str, to: &str, status: Option<u16> ) -> RewriteRule {

    RewriteRule { from: from.to_string(), to: to.to_string(), status }

}

fn api ( rewrite: Vec<RewriteRule>, strip: bool ) -> Route {

    Route { name: "api".to_string(), path: "/api".to_string(), upstream: "default".to_string(), strip_prefix: strip, rewrite, ..Route::default() }

}

#[test]
fn rewrites_paths_before_forwarding_and_keeps_the_query () {

    let origin = Origin::start();
    let running = proxy(origin.addr, |config| config.routes.push(api(vec![rule("^/api/v1/(.*)$", "/v2/$1", None)], false)));
    let mut client = Http1::connect(running.addr());

    assert_eq!(client.get("/api/v1/users?x=1").status, 200);
    assert_eq!(client.get("/api/other?y=2").status, 200);
    assert_eq!(client.get("/api/v1/").status, 200);

    let seen = origin.seen();

    assert_eq!(seen[0].path, "/v2/users?x=1");
    assert_eq!(seen[1].path, "/api/other?y=2");
    assert_eq!(seen[2].path, "/v2/");

    running.stop().expect("stop");

}

#[test]
fn trailing_question_mark_drops_the_query_and_own_queries_merge () {

    let origin = Origin::start();
    let running = proxy(origin.addr, |config| {
        config.routes.push(api(vec![rule("^/api/drop/(.*)$", "/plain/$1?", None), rule("^/api/merge/(.*)$", "/s/$1?mode=fast", None)], false));
    });
    let mut client = Http1::connect(running.addr());

    assert_eq!(client.get("/api/drop/a?x=1").status, 200);
    assert_eq!(client.get("/api/merge/b?x=1").status, 200);
    assert_eq!(client.get("/api/merge/c").status, 200);

    let seen = origin.seen();

    assert_eq!(seen[0].path, "/plain/a");
    assert_eq!(seen[1].path, "/s/b?mode=fast&x=1");
    assert_eq!(seen[2].path, "/s/c?mode=fast");

    running.stop().expect("stop");

}

#[test]
fn rules_chain_in_order_on_the_rewritten_path () {

    let origin = Origin::start();
    let running = proxy(origin.addr, |config| config.routes.push(api(vec![rule("^/api/a/(.*)$", "/api/b/$1", None), rule("^/api/b/(.*)$", "/api/c/$1", None)], false)));
    let mut client = Http1::connect(running.addr());

    assert_eq!(client.get("/api/a/x").status, 200);
    assert_eq!(client.get("/api/b/y").status, 200);

    let seen = origin.seen();

    assert_eq!(seen[0].path, "/api/c/x");
    assert_eq!(seen[1].path, "/api/c/y");

    running.stop().expect("stop");

}

#[test]
fn absolute_targets_and_explicit_statuses_redirect () {

    let origin = Origin::start();
    let running = proxy(origin.addr, |config| {
        config.routes.push(api(vec![rule("^/api/away/(.*)$", "https://example.com/$1", None), rule("^/api/moved/(.*)$", "/new/$1", Some(301))], false));
    });
    let mut client = Http1::connect(running.addr());

    let away = client.get("/api/away/users?x=1");

    assert_eq!(away.status, 302);
    assert_eq!(away.header("location"), Some("https://example.com/users?x=1"));
    assert_eq!(away.header("content-length"), Some("0"));

    let moved = client.get("/api/moved/thing");

    assert_eq!(moved.status, 301);
    assert_eq!(moved.header("location"), Some("/new/thing"));

    let kept = client.get("/api/plain");

    assert_eq!(kept.status, 200);
    assert_eq!(origin.seen().len(), 1);

    running.stop().expect("stop");

}

#[test]
fn strip_prefix_is_skipped_once_a_rewrite_changed_the_path () {

    let origin = Origin::start();
    let running = proxy(origin.addr, |config| config.routes.push(api(vec![rule("^/api/v1/(.*)$", "/v2/$1", None)], true)));
    let mut client = Http1::connect(running.addr());

    assert_eq!(client.get("/api/v1/users").status, 200);
    assert_eq!(client.get("/api/plain?k=v").status, 200);

    let seen = origin.seen();

    assert_eq!(seen[0].path, "/v2/users");
    assert_eq!(seen[1].path, "/plain?k=v");

    running.stop().expect("stop");

}

#[test]
fn invalid_patterns_fail_at_boot () {

    let origin = Origin::start();
    let mut config = Config { listen: support::free_port(), ..Config::default() };

    config.set_upstream(origin.addr);
    config.runtime.workers = 1;
    config.runtime.pin = false;
    config.routes.push(api(vec![rule("^/api/(unclosed", "/x", None)], false));

    let error = aegisx::app::Boot::start(config).err().expect("boot must fail").to_string();

    assert!(error.contains("rewrite") && error.contains("api"), "{error}");

}

#[test]
fn lua_accepts_one_rule_or_a_list_and_redirect_only_routes () {

    let config = Config::parse(r#"
        set_upstream("127.0.0.1:3000")
        add_route { name = "one", path = "/one", rewrite = { from = "^/one/(.*)$", to = "/uno/$1" } }
        add_route { name = "many", path = "/many", rewrite = { { from = "^/a$", to = "/b" }, { from = "^/b$", to = "/c?" } } }
        add_route { name = "gone", path = "/gone", upstream = "nowhere", rewrite = { from = "^/gone/(.*)$", to = "https://elsewhere.test/$1", status = 308 } }
    "#, "rewrite.lua", Path::new("/tmp")).expect("config");

    assert_eq!(config.routes[0].rewrite, vec![rule("^/one/(.*)$", "/uno/$1", None)]);
    assert_eq!(config.routes[1].rewrite.len(), 2);
    assert_eq!(config.routes[2].rewrite[0].status, Some(308));

    let bad = Config::parse(r#"
        set_upstream("127.0.0.1:3000")
        add_route { name = "bad", path = "/bad", rewrite = { from = "^/bad$", to = "/good", status = 200 } }
    "#, "rewrite.lua", Path::new("/tmp")).expect_err("status 200 must be rejected").to_string();

    assert!(bad.contains("redirect"), "{bad}");

}
