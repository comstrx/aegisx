mod support;

use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::path::PathBuf;
use std::sync::{Arc, RwLock};
use std::time::Duration;

use aegisx::app::Boot;
use aegisx::config::{AcmeConfig, Config, OnDemandConfig, Route, TlsConfig};
use aegisx::http::tls::{Authority, Demand};
use rustls::pki_types::pem::PemObject;
use rustls::pki_types::{CertificateDer, ServerName};
use rustls::{ClientConfig, ClientConnection, RootCertStore, StreamOwned};
use support::{Origin, proxy};

fn scratch ( label: &str ) -> PathBuf {

    let dir = std::env::temp_dir().join(format!("aegisx-demand-{label}-{}", std::process::id()));

    let _ = std::fs::remove_dir_all(&dir);

    dir

}

fn reaches ( addr: SocketAddr, root_pem: &str, name: &str ) -> bool {

    let mut roots = RootCertStore::empty();

    for cert in CertificateDer::pem_slice_iter(root_pem.as_bytes()) { roots.add(cert.expect("root cert")).expect("root"); }

    let config = Arc::new(ClientConfig::builder().with_root_certificates(roots).with_no_client_auth());
    let session = ClientConnection::new(config, ServerName::try_from(name.to_string()).expect("name")).expect("session");
    let tcp = TcpStream::connect(addr).expect("connect");

    tcp.set_read_timeout(Some(Duration::from_secs(10))).expect("timeout");

    let mut stream = StreamOwned::new(session, tcp);

    if stream.write_all(format!("GET / HTTP/1.1\r\nHost: {name}\r\nConnection: close\r\n\r\n").as_bytes()).is_err() { return false; }

    let mut reply = Vec::new();
    let _ = stream.read_to_end(&mut reply);

    reply.starts_with(b"HTTP/1.1 200")

}

#[test]
fn the_local_authority_issues_listed_names_at_the_handshake () {

    let origin = Origin::start();
    let dir = scratch("listed");
    let tls = TlsConfig { internal: true, ca_dir: dir.join("ca"), on_demand: Some(OnDemandConfig { names: vec!["app.test".to_string(), "*.apps.test".to_string()], capacity: 2, ..OnDemandConfig::default() }), ..TlsConfig::default() };
    let running = proxy(origin.addr, |config| config.tls = Some(tls.clone()));
    let root = std::fs::read_to_string(dir.join("ca/root.pem")).expect("root");

    assert!(reaches(running.addr(), &root, "app.test"));
    assert!(reaches(running.addr(), &root, "one.apps.test"));
    assert!(reaches(running.addr(), &root, "app.test"), "a held certificate is served again");
    assert!(reaches(running.addr(), &root, "localhost"), "the default certificate comes from the authority too");
    assert!(!reaches(running.addr(), &root, "other.test"), "a name outside the list gets no certificate");
    assert!(!reaches(running.addr(), &root, "deep.one.apps.test"), "a wildcard covers one label");
    assert!(!reaches(running.addr(), &root, "two.apps.test"), "the store is full at its capacity");

    running.stop().expect("stop");

    let again = proxy(origin.addr, |config| config.tls = Some(tls.clone()));

    assert_eq!(std::fs::read_to_string(dir.join("ca/root.pem")).expect("root"), root, "the authority survives a restart");
    assert!(reaches(again.addr(), &root, "app.test"));

    again.stop().expect("stop");

    let _ = std::fs::remove_dir_all(&dir);

}

#[test]
fn without_a_list_the_authority_serves_the_configured_hosts () {

    let origin = Origin::start();
    let dir = scratch("hosts");

    let running = proxy(origin.addr, |config| {

        config.tls = Some(TlsConfig { internal: true, ca_dir: dir.join("ca"), ..TlsConfig::default() });
        config.routes.push(Route { name: "site".to_string(), path: "/".to_string(), upstream: "default".to_string(), host: Some("site.test".to_string()), ..Route::default() });
        config.routes.push(Route { name: "rest".to_string(), path: "/".to_string(), upstream: "default".to_string(), ..Route::default() });

    });
    let root = std::fs::read_to_string(dir.join("ca/root.pem")).expect("root");

    assert!(reaches(running.addr(), &root, "site.test"));
    assert!(!reaches(running.addr(), &root, "else.test"));

    running.stop().expect("stop");

    let _ = std::fs::remove_dir_all(&dir);

}

#[test]
fn the_store_mints_once_per_name_and_refuses_what_is_not_a_host () {

    let dir = scratch("store");
    let authority = Authority::open(&dir, 1).expect("authority");
    let demand = Demand::new(vec!["*".to_string()], 2, Some(authority), Arc::new(RwLock::new(HashMap::new())));
    let first = demand.find("a.test").expect("first");

    assert!(Arc::ptr_eq(&first, &demand.find("A.TEST").expect("again")));
    assert!(demand.find("b.test").is_some());
    assert!(demand.find("c.test").is_none(), "capacity");
    assert!(demand.permits("ok-1.example.test"));

    for name in ["10.0.0.1", "::1", "", "bad_name.test", "-edge.test", "double..dot", "white space.test"] { assert!(!demand.permits(name), "{name}"); }

    let listed = Demand::new(vec!["Exact.Test".to_string(), "*.wild.test".to_string()], 8, None, Arc::new(RwLock::new(HashMap::new())));

    assert!(listed.permits("exact.test") && listed.permits("a.wild.test"));
    assert!(!listed.permits("wild.test") && !listed.permits("a.b.wild.test") && !listed.permits("notwild.test"));
    assert!(listed.find("exact.test").is_none(), "without an authority nothing is minted in the handshake");

    let _ = std::fs::remove_dir_all(&dir);

}

#[test]
fn on_demand_settings_are_validated () {

    let dir = scratch("check");
    let check = |tls: TlsConfig| {

        let mut config = Config { tls: Some(tls), ..Config::default() };

        config.set_upstream("127.0.0.1:9".parse::<SocketAddr>().expect("addr"));

        Boot::check(&config).map(|_| ()).map_err(|error| error.to_string())

    };
    let acme = || Some(AcmeConfig { directory: "https://127.0.0.1:1/directory".to_string(), cache_dir: dir.join("acme"), ..AcmeConfig::default() });
    let plan = |names: &[&str], ask: Option<&str>| Some(OnDemandConfig { names: names.iter().map(|name| name.to_string()).collect(), ask: ask.map(str::to_string), ..OnDemandConfig::default() });

    assert!(check(TlsConfig { internal: true, ca_dir: dir.join("ca"), ..TlsConfig::default() }).is_ok());
    assert!(check(TlsConfig { on_demand: plan(&["a.test"], None), ..TlsConfig::default() }).expect_err("no issuer").contains("issuer"));
    assert!(check(TlsConfig { internal: true, acme: acme(), ..TlsConfig::default() }).expect_err("two issuers").contains("two issuers"));
    assert!(check(TlsConfig { acme: acme(), on_demand: plan(&[], None), ..TlsConfig::default() }).expect_err("open issuance").contains("names or ask"));
    assert!(check(TlsConfig { acme: acme(), on_demand: plan(&[], Some("https://ask.test/ok")), ..TlsConfig::default() }).expect_err("ask scheme").contains("http://"));
    assert!(check(TlsConfig { internal: true, ca_dir: dir.join("ca"), on_demand: plan(&["a.test"], Some("http://127.0.0.1:9/ok")), ..TlsConfig::default() }).expect_err("ask without acme").contains("acme"));
    assert!(check(TlsConfig { internal: true, ca_dir: dir.join("ca"), leaf_days: 0, ..TlsConfig::default() }).is_err());

    let _ = std::fs::remove_dir_all(&dir);

}
