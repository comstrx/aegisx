mod support;

use std::path::Path;

use aegisx::config::{Balance, Config};
use aegisx::http::upstream::Protocol;
use support::{Http1, Origin, proxy};

fn parse ( text: &str ) -> Result<Config, String> {

    Config::parse(text, "plain.lua", Path::new("/tmp")).map_err(|error| error.to_string())

}

#[test]
fn plain_words_expand_into_the_same_configuration () {

    let config = parse(r#"
        listen "127.0.0.1:8088"

        upstream "api" { "10.0.0.1:3000", "10.0.0.2:3000", balance = "least_conn", check = "/healthz", slow = "250ms" }
        upstream "php" { "unix:/run/php/php-fpm.sock", fastcgi = true, keepalive = 8 }

        site "shop.test" {
            route "/api" { to = "api", strip = true, limit = "600/m", burst = 20, timeout = "30s", max_body = "10MB" },
            route "/admin" { to = "api", allow = { "10.0.0.0/8" }, auth = { core = "{PLAIN}secret" }, satisfy_any = true },
            route "/" { files = "/srv/shop", spa = true },
        }

        route "/ping" { respond = "pong" }
        route "/gone" { respond = 410, name = "tombstone" }
    "#).expect("config");

    let route = |name: &str| config.routes.iter().find(|route| route.name == name).unwrap_or_else(|| panic!("route {name}"));
    let api = route("shop.test/api");

    assert_eq!(config.listen.port(), 8088);
    assert_eq!(( api.host.as_deref(), api.upstream.as_str(), api.strip_prefix ), ( Some("shop.test"), "api", true ));
    assert_eq!(( api.rate_per_second, api.rate_burst, api.timeout_ms, api.max_body_bytes ), ( Some(10), Some(20), Some(30_000), Some(10 * 1_048_576) ));
    assert_eq!(route("shop.test/admin").acl.allow.len(), 1);
    assert!(route("shop.test/admin").basic_auth.is_some() && route("shop.test/admin").satisfy_any, "every add_route field is accepted next to the short words");
    assert_eq!(route("shop.test/").try_files, vec!["$uri".to_string(), "/index.html".to_string()]);
    assert_eq!(route("/ping").respond.as_ref().map(|respond| ( respond.status, respond.body.as_str() )), Some(( 200, "pong" )));
    assert_eq!(( route("tombstone").host.as_deref(), route("tombstone").respond.as_ref().map(|respond| respond.status) ), ( None, Some(410) ));

    let api = &config.pools["api"];

    assert_eq!(( api.backends.len(), api.options.policy, api.options.slow_ms ), ( 2, Balance::LeastConn, 250 ));
    assert_eq!(api.options.health.as_ref().and_then(|health| health.path.as_deref()), Some("/healthz"));
    assert_eq!(( config.pools["php"].backends[0].protocol, config.pools["php"].options.keepalive ), ( Protocol::Fastcgi, 8 ));

}

#[test]
fn plain_words_say_what_is_wrong () {

    let fails = |text: &str, expected: &str| { let error = parse(&format!("set_upstream(\"127.0.0.1:3000\")\n{text}")).expect_err(text); assert!(error.contains(expected), "{text}: {error}"); };

    fails(r#"route "/x" { timeout = "30 parsecs" }"#, "unknown unit");
    fails(r#"route "/x" { timeout = "soon" }"#, "does not start with a number");
    fails(r#"route "/x" { limit = "fast" }"#, "10/s");
    fails(r#"route "/x" { max_body = -1 }"#, "negative");
    fails(r#"route "/x" { colour = "red" }"#, "colour");
    fails(r#"route "/x" { allow = { "not-a-net" } }"#, "not an address");
    fails(r#"upstream "api" { balance = "least_conn" }"#, "at least one address");
    fails(r#"upstream "api" { "10.0.0.1:3000", colour = "red" }"#, "unknown option `colour`");
    fails(r#"site "a.test" { "not a route" }"#, "route \"/path\"");

}

#[test]
fn a_plain_configuration_serves_requests () {

    let origin = Origin::start();
    let parsed = parse(&format!(r#"
        upstream "app" {{ "{}" }}

        site "test.local" {{
            route "/api" {{ to = "app", limit = "1/s", burst = 0 }},
            route "/hello" {{ respond = "hi" }},
        }}

        route "/" {{ respond = 404 }}
    "#, origin.addr)).expect("config");

    let running = proxy(origin.addr, |config| { config.pools.extend(parsed.pools.clone()); config.routes = parsed.routes.clone(); });
    let mut client = Http1::connect(running.addr());

    assert_eq!(client.get("/hello").text(), "hi");
    assert_eq!([client.get("/api/a").status, client.get("/api/b").status], [200, 429]);
    assert_eq!(client.request("GET", "/api/c", &[( "Host", "other.local" )], b"").status, 404, "a site only answers for its host");
    assert_eq!(origin.seen().len(), 1);

    running.stop().expect("stop");

}
