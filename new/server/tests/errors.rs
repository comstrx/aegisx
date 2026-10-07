mod support;

use std::fs;
use std::path::PathBuf;

use aegisx::config::{BackendConfig, ErrorPage, Route};
use support::{Http1, Origin, free_port, proxy};

fn site ( name: &str ) -> PathBuf {

    let dir = std::env::temp_dir().join(format!("aegisx-errors-{}-{name}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);

    fs::create_dir_all(dir.join("errors")).expect("errors dir");
    fs::write(dir.join("errors/404.html"), "<h1>lost</h1>").expect("404");
    fs::write(dir.join("errors/50x.html"), "<h1>down</h1>").expect("50x");
    fs::write(dir.join("index.html"), "<h1>home</h1>").expect("index");

    dir

}

fn page ( status: &[u16], page: &str, code: Option<u16> ) -> ErrorPage {

    ErrorPage { status: status.to_vec(), page: page.to_string(), code }

}

#[test]
fn local_errors_are_replaced_by_configured_pages () {

    let origin = Origin::start();
    let root = site("local");
    let running = proxy(origin.addr, |config| {
        config.error_pages = vec![page(&[404], "/errors/404.html", None), page(&[502, 503, 504], "/errors/50x.html", None)];
        config.routes.push(Route { name: "errors".to_string(), path: "/errors".to_string(), root: Some(root.clone()), ..Route::default() });
        config.routes.push(Route { name: "api".to_string(), path: "/api".to_string(), upstream: "default".to_string(), ..Route::default() });
        config.routes.push(Route { name: "site".to_string(), path: "/".to_string(), root: Some(root.clone()), ..Route::default() });
    });
    let mut client = Http1::connect(running.addr());

    let missing = client.get("/nothing.html");

    assert_eq!(missing.status, 404);
    assert_eq!(missing.text(), "<h1>lost</h1>");
    assert_eq!(missing.header("content-type"), Some("text/html"));
    assert_eq!(missing.header("content-length"), Some("13"));
    assert!(missing.header("x-request-id").is_some());

    let head = client.request("HEAD", "/nothing.html", &[], b"");

    assert_eq!(head.status, 404);
    assert_eq!(head.header("content-length"), Some("13"));
    assert!(head.body.is_empty());

    let served = client.get("/errors/404.html");

    assert_eq!(served.status, 200);

    let ok = client.get("/api/x");

    assert_eq!(ok.status, 200);
    assert_eq!(ok.text(), "ok");

    running.stop().expect("stop");

    let root = site("down");
    let dead = free_port();
    let running = proxy(dead, |config| {
        config.pools.get_mut("default").expect("pool").backends = vec![BackendConfig { address: dead.into(), ..BackendConfig::default() }];
        config.error_pages = vec![page(&[502, 503, 504], "/errors/50x.html", Some(200))];
        config.routes.push(Route { name: "errors".to_string(), path: "/errors".to_string(), root: Some(root.clone()), ..Route::default() });
        config.routes.push(Route { name: "api".to_string(), path: "/".to_string(), upstream: "default".to_string(), ..Route::default() });
    });
    let mut client = Http1::connect(running.addr());

    let failed = client.get("/api/x");

    assert_eq!(failed.status, 200);
    assert_eq!(failed.text(), "<h1>down</h1>");

    running.stop().expect("stop");

}

#[test]
fn upstream_errors_are_intercepted_only_when_asked () {

    let origin = Origin::start();
    let root = site("intercept");
    let running = proxy(origin.addr, |config| {
        config.routes.push(Route { name: "errors".to_string(), path: "/errors".to_string(), root: Some(root.clone()), ..Route::default() });
        config.routes.push(Route { name: "quiet".to_string(), path: "/quiet".to_string(), upstream: "default".to_string(), strip_prefix: true, error_pages: vec![page(&[404], "/errors/404.html", None)], ..Route::default() });
        config.routes.push(Route { name: "loud".to_string(), path: "/loud".to_string(), upstream: "default".to_string(), strip_prefix: true, intercept_errors: true, error_pages: vec![page(&[404], "/errors/404.html", None), page(&[500], "https://status.example.com/", None)], ..Route::default() });
    });
    let mut client = Http1::connect(running.addr());

    let passed = client.get("/quiet/status/404");

    assert_eq!(passed.status, 404);
    assert_eq!(passed.text(), "ok");

    let replaced = client.get("/loud/status/404");

    assert_eq!(replaced.status, 404);
    assert_eq!(replaced.text(), "<h1>lost</h1>");

    let redirected = client.get("/loud/status/500");

    assert_eq!(redirected.status, 302);
    assert_eq!(redirected.header("location"), Some("https://status.example.com/"));

    let untouched = client.get("/loud/status/503");

    assert_eq!(untouched.status, 503);
    assert_eq!(untouched.text(), "ok");

    running.stop().expect("stop");

}

#[test]
fn error_pages_are_validated_and_lua_accepts_them () {

    let source = |pages: &str| format!(r#"
        set_upstream("127.0.0.1:3000")
        set_error_pages {{ {pages} }}
        add_route {{ name = "api", path = "/" }}
    "#);

    assert!(aegisx::config::Config::parse(&source(r#"{ status = 404, page = "/404.html" }, { status = { 502, 503 }, page = "/50x.html", code = 200 }"#), "ok.lua", std::path::Path::new("/tmp")).is_ok());
    assert!(aegisx::config::Config::parse(&source(r#"{ status = 200, page = "/x.html" }"#), "status.lua", std::path::Path::new("/tmp")).is_err());
    assert!(aegisx::config::Config::parse(&source(r#"{ status = 404, page = "relative.html" }"#), "page.lua", std::path::Path::new("/tmp")).is_err());
    assert!(aegisx::config::Config::parse(&source(r#"{ status = 404, page = "/x.html", code = 99 }"#), "code.lua", std::path::Path::new("/tmp")).is_err());

}
