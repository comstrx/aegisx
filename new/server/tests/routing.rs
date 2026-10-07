use std::path::Path;

use aegisx::app::Snapshot;
use aegisx::config::Config;
use aegisx::http::Request;
use http::header::HeaderMap;
use http::Method;

fn snapshot ( source: &str ) -> Snapshot {

    let config = Config::parse(source, "routing.lua", Path::new("/tmp")).expect("config");

    Snapshot::build(config, 1).expect("snapshot")

}

fn pick ( snapshot: &Snapshot, host: &str, path: &str, method: &str ) -> Option<String> {

    snapshot.route(host, path, &Method::from_bytes(method.as_bytes()).expect("method"), &HeaderMap::new()).map(|route| route.name.to_string())

}

#[test]
fn prefixes_respect_segment_boundaries () {

    let snapshot = snapshot(r#"
        set_upstream("127.0.0.1:3000")
        add_route { name = "api", path = "/api" }
        add_route { name = "root", path = "/" }
    "#);

    assert_eq!(pick(&snapshot, "", "/api", "GET").as_deref(), Some("api"));
    assert_eq!(pick(&snapshot, "", "/api/", "GET").as_deref(), Some("api"));
    assert_eq!(pick(&snapshot, "", "/api/orders/1", "GET").as_deref(), Some("api"));
    assert_eq!(pick(&snapshot, "", "/apix", "GET").as_deref(), Some("root"));
    assert_eq!(pick(&snapshot, "", "/", "GET").as_deref(), Some("root"));

}

#[test]
fn exact_routes_beat_prefix_routes_and_longest_path_wins () {

    let snapshot = snapshot(r#"
        set_upstream("127.0.0.1:3000")
        add_route { name = "health", path = "/health", exact = true }
        add_route { name = "deep", path = "/api/v2/users" }
        add_route { name = "api", path = "/api" }
    "#);

    assert_eq!(pick(&snapshot, "", "/health", "GET").as_deref(), Some("health"));
    assert_eq!(pick(&snapshot, "", "/health/x", "GET"), None);
    assert_eq!(pick(&snapshot, "", "/api/v2/users/7", "GET").as_deref(), Some("deep"));
    assert_eq!(pick(&snapshot, "", "/api/v2", "GET").as_deref(), Some("api"));

}

#[test]
fn hosts_rank_exact_then_wildcard_then_any () {

    let snapshot = snapshot(r#"
        set_upstream("127.0.0.1:3000")
        add_route { name = "exact", host = "app.example.com", path = "/" }
        add_route { name = "wild", host = "*.example.com", path = "/" }
        add_route { name = "any", path = "/" }
        add_route { name = "any-deep", path = "/very/specific/path" }
    "#);

    assert_eq!(pick(&snapshot, "app.example.com", "/x", "GET").as_deref(), Some("exact"));
    assert_eq!(pick(&snapshot, "App.Example.COM:8443", "/x", "GET").as_deref(), Some("exact"));
    assert_eq!(pick(&snapshot, "api.example.com", "/x", "GET").as_deref(), Some("wild"));
    assert_eq!(pick(&snapshot, "a.b.example.com", "/x", "GET").as_deref(), Some("wild"));
    assert_eq!(pick(&snapshot, "example.com", "/x", "GET").as_deref(), Some("any"));
    assert_eq!(pick(&snapshot, "other.net", "/x", "GET").as_deref(), Some("any"));
    assert_eq!(pick(&snapshot, "app.example.com", "/very/specific/path", "GET").as_deref(), Some("exact"));

}

#[test]
fn methods_and_headers_filter_candidates () {

    let snapshot = snapshot(r#"
        set_upstream("127.0.0.1:3000")
        add_route { name = "write", path = "/items", methods = {"POST", "PUT"} }
        add_route { name = "preview", path = "/items", match_headers = { ["x-release"] = "preview" } }
        add_route { name = "read", path = "/items" }
    "#);

    assert_eq!(pick(&snapshot, "", "/items", "POST").as_deref(), Some("write"));
    assert_eq!(pick(&snapshot, "", "/items", "GET").as_deref(), Some("read"));

    let mut headers = HeaderMap::new();

    headers.insert("x-release", "preview".parse().expect("value"));

    let found = snapshot.route("", "/items", &Method::GET, &headers).map(|route| route.name.to_string());

    assert_eq!(found.as_deref(), Some("preview"));

}

#[test]
fn policies_inherit_and_override () {

    let snapshot = snapshot(r#"
        set_upstream("127.0.0.1:3000")
        set_limits { timeout_ms = 5000, max_body_bytes = 1000 }
        set_headers("request", { ["x-global"] = "1", ["x-shared"] = "global" })
        add_route { name = "a", path = "/a", timeout_ms = 300, request_headers = { ["x-shared"] = "route" }, strip_prefix = true }
        add_route { name = "b", path = "/b" }
    "#);

    let a = snapshot.route("", "/a/x", &Method::GET, &HeaderMap::new()).expect("a");
    let b = snapshot.route("", "/b", &Method::GET, &HeaderMap::new()).expect("b");

    assert_eq!(a.plan.timeout_ms, 300);
    assert_eq!(b.plan.timeout_ms, 5000);
    assert_eq!(a.plan.max_body_bytes, 1000);
    assert_eq!(a.plan.mount.len(), 2);
    assert_eq!(b.plan.mount.len(), 0);

    let shared = a.plan.request_headers.iter().find(|( name, _ )| name == "x-shared").and_then(|( _, value )| match value { aegisx::http::header::Rendered::Static(value) => value.to_str().ok(), _ => None });

    assert_eq!(shared, Some("route"));
    assert_eq!(a.plan.request_headers.len(), 2);

}

#[test]
fn canonical_paths_normalize_and_reject_ambiguity () {

    assert_eq!(Request::canonical("/plain/path").as_deref(), Some("/plain/path"));
    assert_eq!(Request::canonical("/a%41b").as_deref(), Some("/aAb"));
    assert_eq!(Request::canonical("/sp%20ace").as_deref(), Some("/sp%20ace"));
    assert_eq!(Request::canonical("/x/../y"), None);
    assert_eq!(Request::canonical("/x//y"), None);
    assert_eq!(Request::canonical("/x/."), None);
    assert_eq!(Request::canonical("/x%2Fy"), None);
    assert_eq!(Request::canonical("/x%2fy"), None);
    assert_eq!(Request::canonical("/x%00"), None);
    assert_eq!(Request::canonical("/x%zz"), None);
    assert_eq!(Request::canonical("/x\\y"), None);
    assert_eq!(Request::canonical("/x;y"), None);
    assert_eq!(Request::canonical("relative"), None);
    assert_eq!(Request::canonical("/ok%7Etilde").as_deref(), Some("/ok~tilde"));

}

#[test]
fn host_names_drop_ports_and_brackets () {

    assert_eq!(Request::host_name("example.com:8080"), "example.com");
    assert_eq!(Request::host_name("example.com"), "example.com");
    assert_eq!(Request::host_name("[::1]:8080"), "::1");
    assert_eq!(Request::host_name("[::1]"), "::1");

}
