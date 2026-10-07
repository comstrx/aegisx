mod support;

use std::path::Path;

use aegisx::config::{Acl, Config, Route};
use ipnet::IpNet;
use support::{Http1, Origin, proxy};

fn nets ( list: &[&str] ) -> Vec<IpNet> {

    list.iter().map(|text| text.parse().expect("network")).collect()

}

fn route ( name: &str, path: &str, acl: Acl ) -> Route {

    Route { name: name.to_string(), path: path.to_string(), upstream: "default".to_string(), acl, ..Route::default() }

}

#[test]
fn routes_admit_only_the_listed_networks () {

    let origin = Origin::start();
    let running = proxy(origin.addr, |config| {

        config.routes.push(route("private", "/private", Acl { allow: nets(&["10.0.0.0/8"]), deny: Vec::new() }));
        config.routes.push(route("local", "/local", Acl { allow: nets(&["127.0.0.0/8", "10.0.0.0/8"]), deny: Vec::new() }));
        config.routes.push(route("banned", "/banned", Acl { allow: Vec::new(), deny: nets(&["127.0.0.1/32"]) }));
        config.routes.push(route("open", "/", Acl::default()));

    });
    let mut client = Http1::connect(running.addr());

    assert_eq!(client.get("/private/x").status, 403);
    assert_eq!(client.get("/local/x").status, 200);
    assert_eq!(client.get("/banned/x").status, 403);
    assert_eq!(client.get("/open").status, 200);
    assert_eq!(origin.seen().len(), 2);

    running.stop().expect("stop");

}

#[test]
fn the_global_list_applies_to_every_route_and_a_route_allow_list_replaces_the_global_one () {

    let origin = Origin::start();
    let running = proxy(origin.addr, |config| {

        config.acl = Acl { allow: nets(&["192.0.2.0/24"]), deny: Vec::new() };
        config.routes.push(route("local", "/local", Acl { allow: nets(&["127.0.0.1/32"]), deny: Vec::new() }));
        config.routes.push(route("rest", "/", Acl::default()));

    });
    let mut client = Http1::connect(running.addr());

    assert_eq!(client.get("/local").status, 200);
    assert_eq!(client.get("/anything").status, 403);

    running.stop().expect("stop");

}

#[test]
fn trusted_proxies_reveal_the_client_address () {

    let origin = Origin::start();
    let running = proxy(origin.addr, |config| {

        config.identity.trusted_peers = nets(&["127.0.0.0/8"]);
        config.routes.push(route("partners", "/", Acl { allow: nets(&["203.0.113.0/24"]), deny: nets(&["203.0.113.66/32"]) }));

    });
    let mut client = Http1::connect(running.addr());

    assert_eq!(client.request("GET", "/", &[( "x-forwarded-for", "203.0.113.7" )], b"").status, 200);
    assert_eq!(client.request("GET", "/", &[( "x-forwarded-for", "198.51.100.1, 203.0.113.9, 127.0.0.1" )], b"").status, 200);
    assert_eq!(client.request("GET", "/", &[( "x-forwarded-for", "203.0.113.66" )], b"").status, 403);
    assert_eq!(client.request("GET", "/", &[( "x-forwarded-for", "198.51.100.1" )], b"").status, 403);
    assert_eq!(client.get("/").status, 403);

    running.stop().expect("stop");

}

#[test]
fn lists_parse_networks_and_bare_addresses () {

    let config = Config::parse(r#"
        set_upstream("127.0.0.1:3000")
        set_acl { deny = { "192.0.2.0/24", "198.51.100.7", "2001:db8::/32" } }
        add_route { name = "admin", path = "/admin", acl = { allow = { "10.0.0.0/8", "::1" } } }
        add_route { name = "all", path = "/" }
    "#, "acl.lua", Path::new("/tmp")).expect("parse");

    assert_eq!(config.acl.deny, nets(&["192.0.2.0/24", "198.51.100.7/32", "2001:db8::/32"]));
    assert_eq!(config.routes[0].acl.allow, nets(&["10.0.0.0/8", "::1/128"]));

    let error = Config::parse(r#"
        set_upstream("127.0.0.1:3000")
        set_acl { deny = { "not-a-network" } }
    "#, "acl.lua", Path::new("/tmp")).expect_err("invalid network").to_string();

    assert!(error.contains("not-a-network"), "{error}");

}
