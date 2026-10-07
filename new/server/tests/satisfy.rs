mod support;

use std::collections::BTreeMap;

use aegisx::config::{Acl, BasicAuth, Config, Route};
use base64::Engine;
use support::{Http1, Origin, proxy};

fn guarded ( name: &str, path: &str, allow: &str, any: bool ) -> Route {

    let users = BTreeMap::from([( "core".to_string(), "{PLAIN}secret".to_string() )]);

    Route { name: name.to_string(), path: path.to_string(), upstream: "default".to_string(), acl: Acl { allow: vec![allow.parse().expect("net")], deny: Vec::new() }, basic_auth: Some(BasicAuth { realm: "ops".to_string(), users, users_file: None }), satisfy_any: any, ..Route::default() }

}

#[test]
fn satisfy_any_lets_an_address_or_a_login_through () {

    let origin = Origin::start();
    let running = proxy(origin.addr, |config| {

        config.routes.push(guarded("far-any", "/far-any", "10.0.0.0/8", true));
        config.routes.push(guarded("near-any", "/near-any", "127.0.0.0/8", true));
        config.routes.push(guarded("far-all", "/far-all", "10.0.0.0/8", false));
        config.routes.push(guarded("near-all", "/near-all", "127.0.0.0/8", false));
        config.routes.push(Route { name: "rest".to_string(), path: "/".to_string(), upstream: "default".to_string(), ..Route::default() });

    });
    let mut client = Http1::connect(running.addr());
    let login = format!("Basic {}", base64::engine::general_purpose::STANDARD.encode("core:secret"));
    let mut call = |path: &str, signed: bool| client.request("GET", path, &signed.then_some(( "Authorization", login.as_str() )).into_iter().collect::<Vec<_>>(), b"").status;

    assert_eq!([call("/far-any", false), call("/far-any", true)], [401, 200], "outside the list a login is asked for and is enough");
    assert_eq!(call("/near-any", false), 200, "inside the list no login is needed");
    assert_eq!([call("/far-all", false), call("/far-all", true)], [403, 403], "by default both must hold");
    assert_eq!([call("/near-all", false), call("/near-all", true)], [401, 200]);

    running.stop().expect("stop");

}

#[test]
fn routes_match_on_the_scheme_of_the_connection () {

    let origin = Origin::start();
    let running = proxy(origin.addr, |config| {

        config.routes.push(Route { name: "tls-only".to_string(), path: "/secure".to_string(), upstream: "default".to_string(), scheme: Some("https".to_string()), ..Route::default() });
        config.routes.push(Route { name: "plain-only".to_string(), path: "/plain".to_string(), upstream: "default".to_string(), scheme: Some("http".to_string()), ..Route::default() });
        config.routes.push(Route { name: "closed".to_string(), path: "/".to_string(), upstream: "default".to_string(), deny: true, ..Route::default() });

    });
    let mut client = Http1::connect(running.addr());

    assert_eq!(client.get("/plain/a").status, 200);
    assert_eq!(client.get("/secure/a").status, 403, "an https-only route does not match a plain connection");
    assert!(Config::parse(r#"set_upstream("127.0.0.1:3000") add_route { name = "r", path = "/", scheme = "ftp" }"#, "scheme.lua", std::path::Path::new("/tmp")).is_err());

    running.stop().expect("stop");

}
