mod support;

use aegisx::config::{Config, HookSpec};
use support::{Http1, Origin, proxy};

const SCRIPT: &str = r#"
on_request(function(req)

    if req.headers["x-block"] then return { status = 403, body = "blocked " .. req.method .. " from " .. req.ip, headers = { ["x-why"] = "hook" } } end

    if req.path == "/boom" then error("kaboom") end

    if req.path == "/spin" then while true do end end

    req.headers["x-seen"] = req.scheme .. "://" .. req.host .. req.path .. "?" .. req.query
    req.headers["x-drop"] = nil

end)

on_response(function(req, res)

    res.headers["x-hooked"] = req.method .. " " .. req.path .. " " .. res.status

    if req.path == "/teapot" then res.status = 418 end

end)
"#;

fn hooked ( config: &mut Config ) {

    config.hooks = HookSpec { request: true, response: true, source: SCRIPT.to_string(), name: "hooks.lua".to_string(), base: std::env::temp_dir() };

}

#[test]
fn hooks_edit_requests_and_responses_and_can_answer_early () {

    let origin = Origin::start();
    let running = proxy(origin.addr, hooked);
    let mut client = Http1::connect(running.addr());

    let plain = client.request("GET", "/items?page=2", &[( "X-Drop", "secret" ), ( "X-Keep", "yes" )], b"");

    assert_eq!(plain.status, 200);
    assert_eq!(plain.header("x-hooked"), Some("GET /items 200"));

    let seen = origin.seen();

    assert_eq!(seen[0].header("x-seen"), Some("http://test.local/items?page=2"));
    assert_eq!(seen[0].header("x-drop"), None);
    assert_eq!(seen[0].header("x-keep"), Some("yes"));

    let blocked = client.request("POST", "/items", &[( "X-Block", "1" )], b"payload");

    assert_eq!(blocked.status, 403);
    assert_eq!(blocked.text(), "blocked POST from 127.0.0.1");
    assert_eq!(blocked.header("x-why"), Some("hook"));
    assert_eq!(origin.seen().len(), 0, "an early answer never reaches the upstream");
    assert_eq!(client.get("/teapot").status, 418);

    running.stop().expect("stop");

}

#[test]
fn a_failing_or_endless_hook_costs_one_request_not_the_worker () {

    let origin = Origin::start();
    let running = proxy(origin.addr, hooked);
    let mut client = Http1::connect(running.addr());

    assert_eq!(client.get("/boom").status, 500);
    assert_eq!(client.get("/spin").status, 500);
    assert_eq!(client.get("/after").status, 200);
    assert_eq!(origin.seen().len(), 1);

    running.stop().expect("stop");

}

#[test]
fn the_dsl_records_which_hooks_a_file_declares () {

    let dir = std::path::Path::new("/tmp");
    let both = Config::parse(&format!("set_upstream(\"127.0.0.1:3000\")\n{SCRIPT}"), "hooks.lua", dir).expect("config");

    assert!(both.hooks.request && both.hooks.response);
    assert!(both.hooks.source.contains("on_response") && both.hooks.name == "hooks.lua");

    let none = Config::parse(r#"set_upstream("127.0.0.1:3000")"#, "plain.lua", dir).expect("config");

    assert!(!none.hooks.request && !none.hooks.response && none.hooks.source.is_empty());
    assert!(Config::parse(r#"set_upstream("127.0.0.1:3000") on_request("not a function")"#, "bad.lua", dir).is_err());

}
