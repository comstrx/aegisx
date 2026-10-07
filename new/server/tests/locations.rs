mod support;

use std::path::Path;

use aegisx::app::Snapshot;
use aegisx::config::Config;
use http::Method;
use http::header::HeaderMap;

fn snapshot ( source: &str ) -> Snapshot {

    let config = Config::parse(source, "locations.lua", Path::new("/tmp")).expect("config");

    Snapshot::build(config, 1).expect("snapshot")

}

fn pick ( snapshot: &Snapshot, host: &str, path: &str ) -> Option<String> {

    snapshot.route(host, path, &Method::GET, &HeaderMap::new()).map(|route| route.name.to_string())

}

#[test]
fn regex_routes_beat_prefix_routes_unless_exact_or_preferred () {

    let snapshot = snapshot(r#"
        set_upstream("127.0.0.1:3000")
        add_route { name = "exact", path = "/images/logo.png", exact = true }
        add_route { name = "guarded", path = "/static", prefer = true }
        add_route { name = "images", regex = [[\.(png|jpe?g)$]] }
        add_route { name = "php", regex = [[(?i)\.php$]] }
        add_route { name = "api", path = "/api" }
        add_route { name = "root", path = "/" }
    "#);

    assert_eq!(pick(&snapshot, "", "/api/photo.png").as_deref(), Some("images"));
    assert_eq!(pick(&snapshot, "", "/api/users").as_deref(), Some("api"));
    assert_eq!(pick(&snapshot, "", "/images/logo.png").as_deref(), Some("exact"));
    assert_eq!(pick(&snapshot, "", "/static/pic.jpg").as_deref(), Some("guarded"));
    assert_eq!(pick(&snapshot, "", "/INDEX.PHP").as_deref(), Some("php"));
    assert_eq!(pick(&snapshot, "", "/plain").as_deref(), Some("root"));

}

#[test]
fn regex_routes_honour_host_scopes_and_config_order () {

    let snapshot = snapshot(r#"
        set_upstream("127.0.0.1:3000")
        add_route { name = "first", regex = "^/a" }
        add_route { name = "second", regex = "^/ab" }
        add_route { name = "scoped", host = "shop.example.com", regex = "^/cart" }
        add_route { name = "wild", host = "*.example.com", regex = "^/wild" }
        add_route { name = "root", path = "/" }
    "#);

    assert_eq!(pick(&snapshot, "", "/abc").as_deref(), Some("first"));
    assert_eq!(pick(&snapshot, "shop.example.com", "/cart").as_deref(), Some("scoped"));
    assert_eq!(pick(&snapshot, "SHOP.EXAMPLE.COM:8443", "/cart").as_deref(), Some("scoped"));
    assert_eq!(pick(&snapshot, "other.example.com", "/cart").as_deref(), Some("root"));
    assert_eq!(pick(&snapshot, "x.example.com", "/wild").as_deref(), Some("wild"));
    assert_eq!(pick(&snapshot, "example.com", "/wild").as_deref(), Some("root"));

}

#[test]
fn invalid_regex_and_conflicting_flags_are_rejected () {

    let bad = Config::parse(r#"set_upstream("127.0.0.1:3000") add_route { name = "x", regex = "(" }"#, "bad.lua", Path::new("/tmp"));

    assert!(bad.is_err());

    let conflict = Config::parse(r#"set_upstream("127.0.0.1:3000") add_route { name = "x", regex = "^/x", exact = true }"#, "conflict.lua", Path::new("/tmp"));

    assert!(conflict.is_err());

}
