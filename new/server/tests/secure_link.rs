mod support;

use std::time::{SystemTime, UNIX_EPOCH};

use aegisx::app::Link;
use aegisx::config::{Config, Route, SecureLink};
use support::{Http1, Origin, proxy};

const SECRET: &str = "a-long-shared-secret-for-links";

fn spec () -> SecureLink {

    SecureLink { secret: Some(SECRET.to_string()), ..SecureLink::default() }

}

#[test]
fn signed_links_open_until_they_expire_and_only_for_their_path () {

    let origin = Origin::start();
    let running = proxy(origin.addr, |config| {

        config.routes.push(Route { name: "files".to_string(), path: "/files".to_string(), upstream: "default".to_string(), secure_link: Some(spec()), ..Route::default() });
        config.routes.push(Route { name: "rest".to_string(), path: "/".to_string(), upstream: "default".to_string(), ..Route::default() });

    });
    let mut client = Http1::connect(running.addr());
    let link = Link::compile(&spec()).expect("link");
    let now = SystemTime::now().duration_since(UNIX_EPOCH).expect("clock").as_secs();
    let signed = |path: &str, expires: u64| format!("{path}?expires={expires}&sig={}", link.sign(path, expires));

    assert_eq!(client.get(&signed("/files/report.pdf", now + 60)).status, 200);
    assert_eq!(client.get(&signed("/files/report.pdf", now - 5)).status, 410, "a valid signature past its time is gone");
    assert_eq!(client.get(&signed("/files/report.pdf", now + 60).replace("report", "other")).status, 403, "the signature covers the path");
    assert_eq!(client.get(&format!("/files/report.pdf?expires={}&sig={}", now + 600, link.sign("/files/report.pdf", now + 60))).status, 403, "and the expiry");
    assert_eq!(client.get("/files/report.pdf").status, 403);
    assert_eq!(client.get("/files/report.pdf?expires=soon&sig=%%%").status, 403);
    assert_eq!(client.get("/open").status, 200);
    assert_eq!(origin.seen().len(), 2);

    running.stop().expect("stop");

}

#[test]
fn secure_link_settings_are_validated () {

    let build = |option: &str| Config::parse(&format!("set_upstream(\"127.0.0.1:3000\") add_route {{ name = \"r\", path = \"/\", secure_link = {{ {option} }} }}"), "link.lua", std::path::Path::new("/tmp")).and_then(|config| aegisx::app::Snapshot::build(config, 1).map(|_| ()));

    assert!(build(r#"secret = "a-long-shared-secret-for-links", signature = "s", expires = "e""#).is_ok());
    assert!(build(r#"secret = "short""#).is_err());
    assert!(build(r#"secret = "a-long-shared-secret-for-links", secret_env = "AEGISX_LINK_SECRET""#).is_err());
    assert!(build(r#"secret_env = "AEGISX_LINK_SECRET_THAT_IS_NOT_SET""#).is_err());
    assert!(build("").is_err());

}
