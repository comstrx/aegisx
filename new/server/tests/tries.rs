mod support;

use std::fs;
use std::path::{Path, PathBuf};

use aegisx::config::Route;
use support::{Http1, Origin, proxy};

fn site ( name: &str ) -> PathBuf {

    let dir = std::env::temp_dir().join(format!("aegisx-tries-{}-{name}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);

    fs::create_dir_all(dir.join("docs")).expect("docs dir");
    fs::write(dir.join("index.html"), "<h1>spa</h1>").expect("index");
    fs::write(dir.join("docs/index.html"), "<h1>docs</h1>").expect("docs index");
    fs::write(dir.join("app.js"), "console.log(1)").expect("app.js");
    fs::write(dir.join("fallback.html"), "<h1>fallback</h1>").expect("fallback");

    dir

}

fn route ( root: &Path, tries: &[&str], upstream: &str ) -> Route {

    Route { name: "site".to_string(), path: "/".to_string(), root: Some(root.to_path_buf()), upstream: upstream.to_string(), try_files: tries.iter().map(|entry| entry.to_string()).collect(), ..Route::default() }

}

#[test]
fn try_files_serves_the_first_existing_candidate_then_the_fallback_file () {

    let origin = Origin::start();
    let root = site("spa");
    let running = proxy(origin.addr, |config| config.routes.push(route(&root, &["$uri", "$uri/", "/index.html"], "default")));
    let mut client = Http1::connect(running.addr());

    assert_eq!(client.get("/app.js").text(), "console.log(1)");
    assert_eq!(client.get("/docs").text(), "<h1>docs</h1>");
    assert_eq!(client.get("/docs/").text(), "<h1>docs</h1>");
    assert_eq!(client.get("/users/42").text(), "<h1>spa</h1>");
    assert_eq!(client.get("/users/42").status, 200);
    assert_eq!(client.get("/app.js/").text(), "<h1>spa</h1>");
    assert_eq!(origin.seen().len(), 0);

    running.stop().expect("stop");

}

#[test]
fn try_files_can_end_in_a_status_or_hand_over_to_the_upstream () {

    let origin = Origin::start();
    let root = site("status");
    let running = proxy(origin.addr, |config| config.routes.push(route(&root, &["$uri", "=410"], "default")));
    let mut client = Http1::connect(running.addr());

    assert_eq!(client.get("/app.js").status, 200);
    assert_eq!(client.get("/missing").status, 410);
    assert_eq!(client.request("POST", "/missing", &[], b"x").status, 405);

    running.stop().expect("stop");

    let origin = Origin::start();
    let root = site("upstream");
    let running = proxy(origin.addr, |config| config.routes.push(route(&root, &["$uri", "/fallback.html", "@upstream"], "default")));
    let mut client = Http1::connect(running.addr());

    assert_eq!(client.get("/app.js").text(), "console.log(1)");
    assert_eq!(client.get("/anything").text(), "<h1>fallback</h1>");
    assert_eq!(origin.seen().len(), 0);

    running.stop().expect("stop");

    let origin = Origin::start();
    let root = site("handover");
    let running = proxy(origin.addr, |config| config.routes.push(route(&root, &["$uri", "@upstream"], "default")));
    let mut client = Http1::connect(running.addr());

    assert_eq!(client.get("/app.js").text(), "console.log(1)");
    assert_eq!(client.get("/api/users?x=1").text(), "ok");
    assert_eq!(client.request("POST", "/echo", &[], b"payload").text(), "payload");

    let seen = origin.seen();

    assert_eq!(seen.len(), 2);
    assert_eq!(seen[0].path, "/api/users?x=1");

    running.stop().expect("stop");

}

#[test]
fn try_files_configuration_is_validated () {

    let root = site("validate");
    let source = |tries: &str, upstream: &str| format!(r#"
        set_upstream("127.0.0.1:3000")
        add_route {{ name = "site", path = "/", root = "{}", upstream = "{upstream}", try_files = {{ {tries} }} }}
    "#, root.display());

    assert!(aegisx::config::Config::parse(&source(r#""$uri", "@upstream", "/x""#, "default"), "a.lua", Path::new("/tmp")).is_err());
    assert!(aegisx::config::Config::parse(&source(r#""$uri", "=99""#, "default"), "b.lua", Path::new("/tmp")).is_err());
    assert!(aegisx::config::Config::parse(&source(r#""$uri", "@upstream""#, "ghost"), "c.lua", Path::new("/tmp")).is_err());
    assert!(aegisx::config::Config::parse(&source(r#""$uri", "@upstream""#, "default"), "d.lua", Path::new("/tmp")).is_ok());

}
