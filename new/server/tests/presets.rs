mod support;

use std::path::Path;

use aegisx::config::Config;
use aegisx::http::upstream::Protocol;
use support::{Http1, Origin, proxy};

#[test]
fn presets_expand_into_plain_routes_and_pools () {

    let config = Config::parse(r#"
        set_upstream("127.0.0.1:3000")
        static_site { host = "docs.test", root = "/srv/docs", cache_control = "public, max-age=600" }
        spa { host = "app.test", root = "/srv/app" }
        php_app { host = "shop.test", root = "/srv/shop/public", fastcgi = "unix:/run/php/php-fpm.sock" }
    "#, "presets.lua", Path::new("/tmp")).expect("config");

    let route = |name: &str| config.routes.iter().find(|route| route.name == name).unwrap_or_else(|| panic!("route {name}"));

    assert_eq!(route("static_site:docs.test/").root.as_deref(), Some(Path::new("/srv/docs")));
    assert_eq!(route("static_site:docs.test/").cache_control.as_deref(), Some("public, max-age=600"));
    assert_eq!(route("spa:app.test/").try_files, vec!["$uri".to_string(), "/index.html".to_string()]);
    assert_eq!(route("php_app:shop.test/").upstream, "php:shop.test");
    assert_eq!(config.pools["php:shop.test"].backends[0].protocol, Protocol::Fastcgi);

    let parse = |text: &str| Config::parse(&format!("set_upstream(\"127.0.0.1:3000\") {text}"), "presets.lua", Path::new("/tmp"));

    assert!(parse(r#"spa { host = "app.test" }"#).expect_err("root").to_string().contains("root"));
    assert!(parse(r#"php_app { root = "/srv/app" }"#).expect_err("fastcgi").to_string().contains("fastcgi"));
    assert!(parse(r#"static_site { root = "/srv/app", colour = "red" }"#).is_err());

}

#[test]
fn a_single_page_app_falls_back_to_its_index () {

    let origin = Origin::start();
    let dir = std::env::temp_dir().join(format!("aegisx-spa-{}", std::process::id()));

    std::fs::create_dir_all(dir.join("assets")).expect("dir");
    std::fs::write(dir.join("index.html"), b"<main>app</main>").expect("index");
    std::fs::write(dir.join("assets/app.js"), b"run()").expect("script");

    let parsed = Config::parse(&format!(r#"set_upstream("{}") spa {{ root = "{}" }}"#, origin.addr, dir.display()), "spa.lua", Path::new("/tmp")).expect("config");
    let running = proxy(origin.addr, |config| config.routes = parsed.routes.clone());
    let mut client = Http1::connect(running.addr());

    assert_eq!(client.get("/assets/app.js").text(), "run()");
    assert_eq!(client.get("/users/7/settings").text(), "<main>app</main>");
    assert_eq!(client.get("/").text(), "<main>app</main>");
    assert_eq!(origin.seen().len(), 0);

    running.stop().expect("stop");

    let _ = std::fs::remove_dir_all(&dir);

}
