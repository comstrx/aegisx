mod support;

use std::collections::BTreeMap;
use std::path::Path;

use aegisx::config::{Config, Route};
use support::{Http1, Origin, proxy};

#[test]
fn header_rules_set_append_default_and_remove_in_both_directions () {

    let origin = Origin::start();
    let running = proxy(origin.addr, |config| {

        config.request_headers.insert("-x-secret".to_string(), String::new());
        config.request_headers.insert("+x-tag".to_string(), "edge".to_string());
        config.request_headers.insert("?x-tenant".to_string(), "public".to_string());
        config.response_headers.insert("-content-type".to_string(), String::new());
        config.response_headers.insert("+vary".to_string(), "origin".to_string());
        config.response_headers.insert("?cache-control".to_string(), "no-store".to_string());
        config.routes.push(Route { name: "all".to_string(), path: "/".to_string(), upstream: "default".to_string(), response_headers: BTreeMap::from([( "x-frame-options".to_string(), "DENY".to_string() )]), ..Route::default() });

    });
    let mut client = Http1::connect(running.addr());

    let reply = client.request("GET", "/", &[( "x-secret", "token" ), ( "x-tag", "client" ), ( "x-tenant", "acme" )], b"");

    assert_eq!(reply.status, 200);
    assert_eq!(reply.header("content-type"), None);
    assert_eq!(reply.header("vary"), Some("origin"));
    assert_eq!(reply.header("cache-control"), Some("no-store"));
    assert_eq!(reply.header("x-frame-options"), Some("DENY"));

    let plain = client.get("/");

    assert_eq!(plain.status, 200);

    let seen = origin.seen();

    assert_eq!(seen[0].header("x-secret"), None);
    assert_eq!(seen[0].header("x-tenant"), Some("acme"));
    assert!(seen[0].headers.iter().filter(|( name, _ )| name.eq_ignore_ascii_case("x-tag")).map(|( _, value )| value.as_str()).eq(["client", "edge"]));
    assert_eq!(seen[1].header("x-tenant"), Some("public"));
    assert_eq!(seen[1].header("x-tag"), Some("edge"));

    running.stop().expect("stop");

}

#[test]
fn header_rules_are_validated () {

    let parse = |source: &str| Config::parse(source, "headers.lua", Path::new("/tmp")).map(|_| ()).map_err(|error| error.to_string());

    assert!(parse(r#"set_upstream("127.0.0.1:3000") set_headers("response", { ["-server"] = "", ["+vary"] = "origin", ["?cache-control"] = "no-store" })"#).is_ok());
    assert!(parse(r#"set_upstream("127.0.0.1:3000") set_headers("response", { ["-server"] = "nginx" })"#).expect_err("removal with a value").contains("removal"));
    assert!(parse(r#"set_upstream("127.0.0.1:3000") set_headers("request", { ["-host"] = "" })"#).expect_err("host removal").contains("Host"));

    let dynamic = Config::parse(r#"set_upstream("127.0.0.1:3000") set_headers("request", { ["+x-peer"] = "$remote_addr" })"#, "headers.lua", Path::new("/tmp")).map(|config| aegisx::app::Snapshot::build(config, 1).map(|_| ()).map_err(|error| error.to_string())).expect("parse");

    assert!(dynamic.expect_err("variable with append").contains("variable"));

}
