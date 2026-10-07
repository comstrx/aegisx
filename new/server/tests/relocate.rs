mod support;

use aegisx::config::{Replacement, Route};
use support::{Http1, Origin, proxy};

fn edit ( from: &str, to: &str ) -> Replacement {

    Replacement { from: from.to_string(), to: to.to_string() }

}

#[test]
fn default_redirect_rewriting_strips_the_upstream_authority_and_restores_the_mount () {

    let origin = Origin::start();
    let running = proxy(origin.addr, |config| {
        config.routes.push(Route { name: "app".to_string(), path: "/app".to_string(), upstream: "default".to_string(), strip_prefix: true, redirects: Some(Vec::new()), ..Route::default() });
        config.routes.push(Route { name: "raw".to_string(), path: "/".to_string(), upstream: "default".to_string(), ..Route::default() });
    });
    let mut client = Http1::connect(running.addr());

    let reply = client.get("/app/redirect");

    assert_eq!(reply.status, 302);
    assert_eq!(reply.header("location"), Some("/app/landing?x=1"));
    assert_eq!(reply.header("refresh"), Some("0; url=/app/again"));

    let reply = client.get("/redirect");

    assert_eq!(reply.status, 302);
    assert_eq!(reply.header("location"), Some(format!("http://{}/landing?x=1", origin.addr).as_str()));

    running.stop().expect("stop");

}

#[test]
fn explicit_redirect_rules_replace_prefixes_in_order () {

    let origin = Origin::start();
    let running = proxy(origin.addr, |config| {
        let from = format!("http://{}/", origin.addr);
        config.routes.push(Route { name: "app".to_string(), path: "/".to_string(), upstream: "default".to_string(), redirects: Some(vec![edit("http://nothing.local/", "/nope/"), edit(&from, "https://public.example.com/")]), ..Route::default() });
    });
    let mut client = Http1::connect(running.addr());

    let reply = client.get("/redirect");

    assert_eq!(reply.status, 302);
    assert_eq!(reply.header("location"), Some("https://public.example.com/landing?x=1"));
    assert_eq!(reply.header("refresh"), Some("0; url=https://public.example.com/again"));

    running.stop().expect("stop");

}

#[test]
fn cookie_domain_and_path_rewrites_touch_only_the_named_attributes () {

    let origin = Origin::start();
    let running = proxy(origin.addr, |config| {
        config.routes.push(Route {
            name          : "app".to_string(),
            path          : "/".to_string(),
            upstream      : "default".to_string(),
            cookie_domain : vec![edit("backend.local", "example.com")],
            cookie_path   : vec![edit("/app/", "/"), edit("/other", "/elsewhere")],
            ..Route::default()
        });
    });
    let mut client = Http1::connect(running.addr());

    let reply = client.get("/cookie");

    assert_eq!(reply.status, 200);

    let cookies: Vec<&str> = reply.headers.iter().filter(|( name, _ )| name.eq_ignore_ascii_case("set-cookie")).map(|( _, value )| value.as_str()).collect();

    assert_eq!(cookies, vec!["sid=1; Path=/; Domain=example.com; HttpOnly", "theme=dark; path=/elsewhere; domain=example.com"]);

    running.stop().expect("stop");

}
