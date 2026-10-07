mod support;

use std::collections::BTreeMap;
use std::io::Write;
use std::net::TcpStream;
use std::sync::Arc;

use aegisx::config::{Config, Route, TlsConfig};
use rustls::pki_types::pem::PemObject;
use rustls::pki_types::{CertificateDer, ServerName};
use rustls::{ClientConfig, ClientConnection, RootCertStore, StreamOwned};
use support::{Http1, Origin, material, proxy};

fn route ( name: &str, path: &str ) -> Route {

    Route { name: name.to_string(), path: path.to_string(), upstream: "default".to_string(), ..Route::default() }

}

#[test]
fn a_leading_bang_negates_a_match_and_guards_referers () {

    let origin = Origin::start();
    let running = proxy(origin.addr, |config| {

        config.routes.push(Route { deny: true, match_headers: BTreeMap::from([( "referer".to_string(), "!~^https://test\\.local/".to_string() )]), ..route("hotlink", "/img") });
        config.routes.push(route("img", "/img"));
        config.routes.push(Route { deny: true, match_query: BTreeMap::from([( "debug".to_string(), "!*".to_string() )]), ..route("needs-debug", "/trace") });
        config.routes.push(route("trace", "/trace"));
        config.routes.push(route("rest", "/"));

    });
    let mut client = Http1::connect(running.addr());
    let fetch = |client: &mut Http1, referer: Option<&str>| client.request("GET", "/img/a.png", &referer.map(|referer| ( "Referer", referer )).into_iter().collect::<Vec<_>>(), b"").status;

    assert_eq!(fetch(&mut client, Some("https://test.local/gallery")), 200);
    assert_eq!(fetch(&mut client, Some("https://elsewhere.test/")), 403);
    assert_eq!(fetch(&mut client, None), 403, "a missing header does not match, so its negation does");
    assert_eq!([client.get("/trace").status, client.get("/trace?debug=1").status], [403, 200]);
    assert!(Config::parse(r#"set_upstream("127.0.0.1:3000") add_route { name = "bad", path = "/", match_headers = { referer = "!~(" } }"#, "gaps.lua", std::path::Path::new("/tmp")).map(|config| aegisx::app::Snapshot::build(config, 1).is_err()).unwrap_or(true));

    running.stop().expect("stop");

}

#[test]
fn routes_can_rewrite_the_method_set_a_charset_and_stay_out_of_the_log () {

    let origin = Origin::start();
    let log = std::env::temp_dir().join(format!("aegisx-gaps-{}.log", std::process::id()));
    let _ = std::fs::remove_file(&log);

    let running = proxy(origin.addr, |config| {

        config.access.path = log.clone();
        config.response_headers.insert("content-type".to_string(), "text/html".to_string());
        config.routes.push(Route { method: Some("GET".to_string()), ..route("as-get", "/as-get") });
        config.routes.push(Route { log: Some(false), ..route("quiet", "/quiet") });
        config.routes.push(Route { charset: Some("utf-8".to_string()), ..route("pages", "/pages") });
        config.routes.push(route("rest", "/"));

    });
    let mut client = Http1::connect(running.addr());

    assert_eq!(client.request("POST", "/as-get", &[], b"").status, 200);
    assert_eq!(origin.seen()[0].method, "GET");
    assert_eq!(client.get("/pages/home").header("content-type"), Some("text/html; charset=utf-8"));
    assert_eq!(client.get("/plain").header("content-type"), Some("text/html"));
    assert_eq!(client.get("/quiet/health").status, 200);

    running.stop().expect("stop");

    let lines = std::fs::read_to_string(&log).expect("log");

    assert!(lines.contains("/pages/home") && lines.contains("/plain"));
    assert!(!lines.contains("/quiet/health"), "a route with log = false writes no line");

    let parse = |option: &str| Config::parse(&format!("set_upstream(\"127.0.0.1:3000\") add_route {{ name = \"r\", path = \"/\", {option} }}"), "gaps.lua", std::path::Path::new("/tmp"));

    assert!(parse(r#"method = "PURGE", charset = "utf-8", log = false"#).is_ok());
    assert!(parse(r#"method = "get""#).is_err());
    assert!(parse(r#"charset = "utf 8""#).is_err());

    let _ = std::fs::remove_file(&log);

}

#[test]
fn the_minimum_tls_version_is_enforced () {

    let origin = Origin::start();
    let material = material(&["localhost"]);
    let secured = |min: &str| TlsConfig { cert: material.cert.clone(), key: material.key.clone(), min_version: min.to_string(), ..TlsConfig::default() };

    let old = |addr: std::net::SocketAddr| {

        let mut roots = RootCertStore::empty();

        for cert in CertificateDer::pem_slice_iter(material.ca_pem.as_bytes()) { roots.add(cert.expect("ca cert")).expect("root"); }

        let config = Arc::new(ClientConfig::builder_with_protocol_versions(&[&rustls::version::TLS12]).with_root_certificates(roots).with_no_client_auth());
        let session = ClientConnection::new(config, ServerName::try_from("localhost").expect("name")).expect("session");
        let mut stream = StreamOwned::new(session, TcpStream::connect(addr).expect("connect"));

        stream.write_all(b"GET / HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n").is_ok() && stream.flush().is_ok()

    };

    let lenient = proxy(origin.addr, |config| config.tls = Some(secured("1.2")));

    assert!(old(lenient.addr()), "TLS 1.2 is accepted by default");

    lenient.stop().expect("stop");

    let strict = proxy(origin.addr, |config| config.tls = Some(secured("1.3")));

    assert!(!old(strict.addr()), "a TLS 1.2 client is refused when the floor is 1.3");
    assert_eq!(Http1::connect_tls(strict.addr(), &material.ca_pem, "localhost").get("/").status, 200);

    strict.stop().expect("stop");

    assert!(aegisx::app::Boot::check(&Config { tls: Some(secured("1.1")), ..Config::default() }).is_err());

}

#[test]
fn an_aborting_route_closes_the_connection_without_an_answer () {

    let origin = Origin::start();
    let running = proxy(origin.addr, |config| {

        config.routes.push(Route { abort: true, ..route("drop", "/drop") });
        config.routes.push(route("rest", "/"));

    });
    let mut client = Http1::connect(running.addr());

    client.send(b"GET /drop/now HTTP/1.1\r\nHost: test.local\r\n\r\n");

    assert!(client.try_reply().is_none(), "no status line is written");
    assert_eq!(Http1::connect(running.addr()).get("/fine").status, 200);
    assert_eq!(origin.seen().len(), 1);

    running.stop().expect("stop");

}

#[test]
fn keyval_entries_are_edited_through_the_api_and_read_as_variables () {

    use aegisx::config::ControlConfig;
    use aegisx::http::variable::{Kind, Recipe};

    const ADMIN: &str = "test-admin-token-0123456789abcdef0123456789abcdef";

    let origin = Origin::start();
    let running = proxy(origin.addr, |config| {

        config.control = ControlConfig { enabled: true, listen: support::free_port(), token_env: "AEGISX_TEST_ADMIN_TOKEN".to_string(), ..ControlConfig::default() };
        config.variables.insert("gaps_plan".to_string(), Recipe { kind: Kind::Keyval, from: "header:x-api-key".to_string(), default: "free".to_string(), ..Recipe::default() });
        config.request_headers.insert("x-plan".to_string(), "$gaps_plan".to_string());
        config.routes.push(Route { deny: true, match_vars: BTreeMap::from([( "gaps_plan".to_string(), "banned".to_string() )]), ..route("banned", "/") });
        config.routes.push(route("rest", "/"));

    });
    let control = running.control_addr().expect("control");
    let host = control.to_string();
    let auth = format!("Bearer {ADMIN}");
    let admin = |method: &str, path: &str, body: &[u8]| Http1::connect(control).request(method, path, &[( "Host", host.as_str() ), ( "Authorization", auth.as_str() ), ( "Content-Type", "application/json" )], body);
    let mut client = Http1::connect(running.addr());
    let mut plan = |key: &str| { let status = client.request("GET", "/", &[( "X-Api-Key", key )], b"").status; ( status, origin.seen().last().and_then(|seen| seen.header("x-plan").map(str::to_owned)) ) };

    assert_eq!(plan("k1"), ( 200, Some("free".to_string()) ));
    assert_eq!(admin("PUT", "/api/v1/keyval/gaps_plan", br#"{"k1":"gold","k2":"banned"}"#).status, 200);
    assert_eq!(plan("k1"), ( 200, Some("gold".to_string()) ));
    assert_eq!(plan("k2").0, 403, "a stored value routes like any other variable");
    assert_eq!(String::from_utf8_lossy(&admin("GET", "/api/v1/keyval/gaps_plan", b"").body).matches("gold").count(), 1);
    assert_eq!(admin("DELETE", "/api/v1/keyval/gaps_plan", br#"{"k2":""}"#).status, 200);
    assert_eq!(plan("k2"), ( 200, Some("free".to_string()) ));
    assert_eq!(admin("PUT", "/api/v1/keyval/unknown", br#"{"a":"b"}"#).status, 404);
    assert_eq!(admin("PUT", "/api/v1/keyval/gaps_plan", b"[1]").status, 400);

    running.stop().expect("stop");

}
