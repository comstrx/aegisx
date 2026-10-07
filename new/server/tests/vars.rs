mod support;

use std::collections::BTreeMap;

use aegisx::config::Route;
use support::{Http1, Origin, proxy};

#[test]
fn header_templates_render_request_and_upstream_variables () {

    let origin = Origin::start();
    let running = proxy(origin.addr, |config| {

        config.request_headers.insert("x-vars".to_string(), "$remote_addr|$host|$request_uri|$uri|$args|$scheme|${server_port}".to_string());
        config.response_headers.insert("x-upstream".to_string(), "$upstream_addr".to_string());
        config.routes.push(Route { name: "all".to_string(), path: "/".to_string(), upstream: "default".to_string(), request_headers: BTreeMap::from([( "x-rid".to_string(), "id:$request_id".to_string() )]), ..Route::default() });

    });
    let port = running.addr().port();
    let mut client = Http1::connect(running.addr());

    let reply = client.get("/path/x?q=1");

    assert_eq!(reply.status, 200);
    assert_eq!(reply.header("x-upstream"), Some(origin.addr.to_string().as_str()));

    let seen = origin.seen();

    assert_eq!(seen[0].header("x-vars"), Some(format!("127.0.0.1|test.local|/path/x?q=1|/path/x|q=1|http|{port}").as_str()));

    let rid = seen[0].header("x-rid").expect("rid");
    let echoed = reply.header("x-request-id").expect("request id");

    assert_eq!(rid, format!("id:{echoed}"));

    running.stop().expect("stop");

}

#[test]
fn unknown_variables_are_rejected () {

    let error = aegisx::config::Config::parse(r#"
        set_upstream("127.0.0.1:3000")
        set_headers("request", { ["x-bad"] = "$nope" })
        add_route { name = "all", path = "/" }
    "#, "vars.lua", std::path::Path::new("/tmp")).map(|config| aegisx::app::Snapshot::build(config, 1).map(|_| ()).map_err(|error| error.to_string())).expect("parse");

    assert!(error.expect_err("unknown variable must fail").contains("nope"));

}

#[test]
fn derived_variables_feed_headers_and_route_matching () {

    use aegisx::http::variable::{Kind, Recipe};

    let origin = Origin::start();
    let running = proxy(origin.addr, |config| {

        let pairs = |items: &[( &str, &str )]| items.iter().map(|( key, value )| ( key.to_string(), value.to_string() )).collect::<std::collections::BTreeMap<_, _>>();

        config.variables.insert("tier".to_string(), Recipe { kind: Kind::Map, from: "header:x-plan".to_string(), values: pairs(&[( "pro", "gold" ), ( "~^ent", "platinum" )]), default: "free".to_string(), ..Recipe::default() });
        config.variables.insert("zone".to_string(), Recipe { kind: Kind::Geo, values: pairs(&[( "127.0.0.0/8", "local" )]), default: "remote".to_string(), ..Recipe::default() });
        config.variables.insert("lane".to_string(), Recipe { kind: Kind::Split, from: "header:x-user".to_string(), buckets: [( "a".to_string(), 50 ), ( "b".to_string(), 50 )].into_iter().collect(), ..Recipe::default() });
        config.routes.push(Route { name: "gold".to_string(), path: "/".to_string(), upstream: "default".to_string(), match_vars: pairs(&[( "tier", "~^(gold|platinum)$" )]), request_headers: pairs(&[( "x-route", "premium" ), ( "x-tier", "$tier" )]), ..Route::default() });
        config.routes.push(Route { name: "search".to_string(), path: "/".to_string(), upstream: "default".to_string(), match_query: pairs(&[( "q", "*" )]), request_headers: pairs(&[( "x-route", "search" )]), ..Route::default() });
        config.routes.push(Route { name: "all".to_string(), path: "/".to_string(), upstream: "default".to_string(), request_headers: pairs(&[( "x-tier", "$tier" ), ( "x-zone", "$zone" ), ( "x-lane", "${lane}" )]), ..Route::default() });

    });
    let mut client = Http1::connect(running.addr());

    assert_eq!(client.request("GET", "/a", &[( "X-Plan", "pro" )], b"").status, 200);
    assert_eq!(client.request("GET", "/b", &[( "X-Plan", "enterprise" )], b"").status, 200);
    assert_eq!(client.get("/c?q=rust").status, 200);
    assert_eq!(client.request("GET", "/d", &[( "X-User", "u-1" )], b"").status, 200);
    assert_eq!(client.request("GET", "/e", &[( "X-User", "u-1" )], b"").status, 200);

    let seen = origin.seen();

    assert_eq!(( seen[0].header("x-route"), seen[0].header("x-tier") ), ( Some("premium"), Some("gold") ));
    assert_eq!(( seen[1].header("x-route"), seen[1].header("x-tier") ), ( Some("premium"), Some("platinum") ));
    assert_eq!(seen[2].header("x-route"), Some("search"));
    assert_eq!(( seen[3].header("x-tier"), seen[3].header("x-zone") ), ( Some("free"), Some("local") ));
    assert!(matches!(seen[3].header("x-lane"), Some("a" | "b")));
    assert_eq!(seen[3].header("x-lane"), seen[4].header("x-lane"));

    running.stop().expect("stop");

    let parsed = aegisx::config::Config::parse(r#"
        set_upstream("127.0.0.1:3000")
        add_map("tier", { from = "header:x-plan", values = { pro = "gold" }, default = "free" })
        add_geo("zone", { values = { ["10.0.0.0/8"] = "office" }, default = "world" })
        add_split("lane", { from = "cookie:uid", buckets = { canary = 5, stable = 95 } })
        add_route { name = "canary", path = "/", match_vars = { lane = "canary" }, match_query = { debug = "1" } }
    "#, "vars.lua", std::path::Path::new("/tmp")).expect("variables parse");

    assert_eq!(parsed.variables.len(), 3);
    assert!(aegisx::config::Config::parse(r#"set_upstream("127.0.0.1:3000") add_map("host", { from = "ip" })"#, "vars.lua", std::path::Path::new("/tmp")).is_err());
    assert!(aegisx::config::Config::parse(r#"set_upstream("127.0.0.1:3000") add_route { name = "r", path = "/", match_vars = { nope = "1" } }"#, "vars.lua", std::path::Path::new("/tmp")).is_err());

}

#[test]
fn bodies_are_rewritten_and_internal_files_follow_an_accel_redirect () {

    let origin = Origin::start();
    let dir = std::env::temp_dir().join(format!("aegisx-accel-{}", std::process::id()));

    std::fs::create_dir_all(dir.join("vault")).expect("dir");
    std::fs::write(dir.join("vault/report.txt"), b"internal report").expect("file");

    let running = proxy(origin.addr, |config| {

        let pairs = |items: &[( &str, &str )]| items.iter().map(|( key, value )| ( key.to_string(), value.to_string() )).collect::<std::collections::BTreeMap<_, _>>();

        config.routes.push(Route { name: "page".to_string(), path: "/echo".to_string(), upstream: "default".to_string(), response_headers: pairs(&[( "content-type", "text/html" )]), replace: pairs(&[( "http://old.test", "https://new.test" ), ( "colour", "color" )]), ..Route::default() });
        config.routes.push(Route { name: "vault".to_string(), path: "/vault".to_string(), root: Some(dir.clone()), internal: true, ..Route::default() });
        config.routes.push(Route { name: "download".to_string(), path: "/download".to_string(), upstream: "default".to_string(), response_headers: pairs(&[( "x-accel-redirect", "/vault/report.txt" ), ( "content-disposition", "attachment" )]), ..Route::default() });
        config.routes.push(Route { name: "all".to_string(), path: "/".to_string(), upstream: "default".to_string(), ..Route::default() });

    });
    let mut client = Http1::connect(running.addr());

    let page = client.request("POST", "/echo", &[], b"<a href=\"http://old.test/x\">colour</a> http://old.test");

    assert_eq!(page.text(), "<a href=\"https://new.test/x\">color</a> https://new.test");
    assert_eq!(client.get("/vault/report.txt").status, 404);

    let file = client.get("/download");

    assert_eq!(( file.status, file.text().as_str() ), ( 200, "internal report" ));
    assert_eq!(file.header("content-disposition"), Some("attachment"));
    assert_eq!(file.header("x-accel-redirect"), None);

    running.stop().expect("stop");

    let _ = std::fs::remove_dir_all(&dir);

}
