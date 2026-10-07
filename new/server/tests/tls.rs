mod support;

use std::io::{Read, Write};
use std::net::TcpStream;
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use rustls::pki_types::{CertificateDer, PrivateKeyDer, ServerName};
use rustls::pki_types::pem::PemObject;
use rustls::{ClientConfig, ClientConnection, HandshakeKind, RootCertStore, StreamOwned};

use aegisx::app::Boot;
use aegisx::config::{ClientAuth, Config, TlsConfig};
use support::{Http1, Material, Origin, material, proxy};

fn secured ( config: &mut Config, cert: &std::path::Path, key: &std::path::Path, handshake_timeout_ms: u64 ) {

    config.tls = Some(TlsConfig { cert: cert.to_path_buf(), key: key.to_path_buf(), handshake_timeout_ms, ..TlsConfig::default() });

}

#[test]
fn terminates_tls_and_reports_https_to_the_backend () {

    let origin = Origin::start();
    let material = material(&["localhost"]);
    let running = proxy(origin.addr, |config| secured(config, &material.cert, &material.key, 5_000));
    let mut client = Http1::connect_tls(running.addr(), &material.ca_pem, "localhost");

    let reply = client.get("/first");

    assert_eq!(reply.status, 200);
    assert_eq!(reply.text(), "ok");

    let seen = origin.seen();

    assert_eq!(seen.len(), 1);
    assert_eq!(seen[0].path, "/first");
    assert_eq!(seen[0].header("x-forwarded-proto"), Some("https"));

    assert_eq!(client.get("/second").status, 200);
    assert_eq!(origin.seen().len(), 1);
    assert_eq!(origin.accepted(), 1);

    running.stop().expect("stop");

}

#[test]
fn plain_http_on_a_tls_listener_is_dropped () {

    let origin = Origin::start();
    let material = material(&["localhost"]);
    let running = proxy(origin.addr, |config| secured(config, &material.cert, &material.key, 5_000));
    let mut client = Http1::connect(running.addr());

    client.send(b"GET / HTTP/1.1\r\nHost: test.local\r\n\r\n");

    assert!(client.try_reply().is_none());
    assert!(origin.seen().is_empty());

    running.stop().expect("stop");

}

#[test]
fn silent_handshakes_are_cut_by_the_timeout () {

    let origin = Origin::start();
    let material = material(&["localhost"]);
    let running = proxy(origin.addr, |config| secured(config, &material.cert, &material.key, 200));
    let mut client = Http1::connect(running.addr());
    let started = Instant::now();

    assert!(client.try_reply().is_none());
    assert!(started.elapsed() < Duration::from_secs(3), "idle handshake held for {:?}", started.elapsed());

    running.stop().expect("stop");

}

#[test]
fn verifies_upstream_tls_against_the_configured_ca () {

    let material = material(&["localhost"]);
    let origin = Origin::start_tls(&material.cert, &material.key);

    let running = proxy(origin.addr, |config| {

        let backend = &mut config.pools.get_mut("default").expect("pool").backends[0];

        backend.tls = true;
        backend.server_name = "localhost".to_string();
        backend.ca_file = Some(material.ca.clone());

    });

    let mut client = Http1::connect(running.addr());

    assert_eq!(client.get("/secure").status, 200);
    assert_eq!(client.get("/again").status, 200);

    let seen = origin.seen();

    assert_eq!(seen.len(), 2);
    assert_eq!(seen[0].header("host"), Some(format!("localhost:{}", origin.addr.port()).as_str()));
    assert_eq!(seen[0].header("x-forwarded-proto"), Some("http"));
    assert_eq!(origin.accepted(), 1);

    running.stop().expect("stop");

}

#[test]
fn rejects_upstream_tls_with_unknown_ca_or_wrong_name () {

    let material = material(&["localhost"]);
    let origin = Origin::start_tls(&material.cert, &material.key);

    let unknown = proxy(origin.addr, |config| {

        let backend = &mut config.pools.get_mut("default").expect("pool").backends[0];

        backend.tls = true;
        backend.server_name = "localhost".to_string();

    });

    assert_eq!(Http1::connect(unknown.addr()).get("/").status, 502);

    unknown.stop().expect("stop");

    let wrong = proxy(origin.addr, |config| {

        let backend = &mut config.pools.get_mut("default").expect("pool").backends[0];

        backend.tls = true;
        backend.server_name = "other.test".to_string();
        backend.ca_file = Some(material.ca.clone());

    });

    assert_eq!(Http1::connect(wrong.addr()).get("/").status, 502);
    assert!(origin.seen().is_empty());

    wrong.stop().expect("stop");

}

#[test]
fn check_rejects_unreadable_or_mismatched_material () {

    let material = support::material(&["localhost"]);
    let other = support::material(&["localhost"]);
    let mut config = Config::default();

    config.set_upstream(std::net::SocketAddr::from(([127, 0, 0, 1], 9)));
    secured(&mut config, &material.dir.join("missing.pem"), &material.key, 5_000);

    assert!(Boot::check(&config).err().expect("missing cert").to_string().contains("cannot read"));

    secured(&mut config, &material.cert, &other.key, 5_000);

    assert!(Boot::check(&config).err().expect("mismatched key").to_string().contains("valid identity"));

    secured(&mut config, &material.cert, &material.key, 5_000);

    assert!(Boot::check(&config).is_ok());

}

#[test]
fn reload_cannot_toggle_tls_but_rotates_material () {

    let origin = Origin::start();
    let material = support::material(&["localhost"]);
    let rotated = support::material(&["localhost"]);
    let running = proxy(origin.addr, |config| secured(config, &material.cert, &material.key, 5_000));

    let mut plain = Config { listen: running.addr(), ..Config::default() };

    plain.set_upstream(origin.addr);
    plain.runtime.workers = 2;
    plain.runtime.pin = false;

    assert!(running.reload(plain.clone()).expect_err("toggle").to_string().contains("restart"));

    let mut next = plain.clone();

    secured(&mut next, &rotated.cert, &rotated.key, 5_000);

    assert_eq!(running.reload(next).expect("rotate"), 2);

    thread::sleep(Duration::from_millis(50));

    let mut client = Http1::connect_tls(running.addr(), &rotated.ca_pem, "localhost");

    assert_eq!(client.get("/").status, 200);

    running.stop().expect("stop");

}

#[test]
fn sni_selects_the_certificate_for_the_requested_name () {

    let origin = Origin::start();
    let primary = support::material(&["a.test"]);
    let secondary = support::material(&["b.test", "*.wild.test"]);

    let running = proxy(origin.addr, |config| {

        secured(config, &primary.cert, &primary.key, 5_000);

        if let Some(tls) = config.tls.as_mut() {

            tls.certificates.push(aegisx::config::Certificate { ocsp: None, names: vec!["b.test".to_string(), "*.wild.test".to_string()], cert: secondary.cert.clone(), key: secondary.key.clone() });

        }

    });

    assert_eq!(Http1::connect_tls(running.addr(), &primary.ca_pem, "a.test").get("/").status, 200);
    assert_eq!(Http1::connect_tls(running.addr(), &secondary.ca_pem, "b.test").get("/").status, 200);
    assert_eq!(Http1::connect_tls(running.addr(), &secondary.ca_pem, "api.wild.test").get("/").status, 200);

    let mut fallback = Http1::connect_tls(running.addr(), &primary.ca_pem, "unknown.test");

    assert!(!fallback.try_send(b"GET / HTTP/1.1\r\nHost: unknown.test\r\n\r\n"), "default certificate should not validate for unknown.test");

    let mut wrong = Http1::connect_tls(running.addr(), &primary.ca_pem, "b.test");

    assert!(!wrong.try_send(b"GET / HTTP/1.1\r\nHost: b.test\r\n\r\n"), "wrong CA accepted the b.test certificate");

    running.stop().expect("stop");

}

fn handshake ( addr: std::net::SocketAddr, config: &Arc<ClientConfig> ) -> ( u16, Option<HandshakeKind> ) {

    let name = ServerName::try_from("localhost".to_string()).expect("server name");
    let session = ClientConnection::new(config.clone(), name).expect("client session");
    let mut stream = StreamOwned::new(session, TcpStream::connect(addr).expect("connect"));

    stream.write_all(b"GET /resume HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n").expect("write");

    let mut raw = Vec::new();
    let _ = stream.read_to_end(&mut raw);
    let text = String::from_utf8_lossy(&raw);
    let status = text.split_whitespace().nth(1).and_then(|code| code.parse::<u16>().ok()).unwrap_or(0);

    ( status, stream.conn.handshake_kind() )

}

fn client ( ca_pem: &str ) -> Arc<ClientConfig> {

    let mut roots = RootCertStore::empty();

    for cert in CertificateDer::pem_slice_iter(ca_pem.as_bytes()) { roots.add(cert.expect("ca cert")).expect("root"); }

    Arc::new(ClientConfig::builder().with_root_certificates(roots).with_no_client_auth())

}

#[test]
fn sessions_resume_with_tickets_and_with_the_server_cache () {

    let origin = Origin::start();
    let material = material(&["localhost"]);

    let running = proxy(origin.addr, |config| secured(config, &material.cert, &material.key, 5_000));
    let config = client(&material.ca_pem);

    assert_eq!(handshake(running.addr(), &config), ( 200, Some(HandshakeKind::Full) ));
    assert_eq!(handshake(running.addr(), &config), ( 200, Some(HandshakeKind::Resumed) ));
    assert_eq!(handshake(running.addr(), &config), ( 200, Some(HandshakeKind::Resumed) ));

    running.stop().expect("stop");

    let running = proxy(origin.addr, |config| {
        secured(config, &material.cert, &material.key, 5_000);
        config.tls.as_mut().expect("tls").tickets = false;
        config.runtime.workers = 1;
    });
    let config = client(&material.ca_pem);

    assert_eq!(handshake(running.addr(), &config), ( 200, Some(HandshakeKind::Full) ));
    assert_eq!(handshake(running.addr(), &config), ( 200, Some(HandshakeKind::Resumed) ));

    running.stop().expect("stop");

    let running = proxy(origin.addr, |config| {
        secured(config, &material.cert, &material.key, 5_000);
        config.tls.as_mut().expect("tls").tickets = false;
        config.tls.as_mut().expect("tls").session_cache = 0;
    });
    let config = client(&material.ca_pem);

    assert_eq!(handshake(running.addr(), &config), ( 200, Some(HandshakeKind::Full) ));
    assert_eq!(handshake(running.addr(), &config), ( 200, Some(HandshakeKind::Full) ));

    running.stop().expect("stop");

}

fn authenticated ( material: &Material, with_certificate: bool ) -> Arc<ClientConfig> {

    let mut roots = RootCertStore::empty();

    for cert in CertificateDer::pem_slice_iter(material.ca_pem.as_bytes()) { roots.add(cert.expect("ca cert")).expect("root"); }

    let builder = ClientConfig::builder().with_root_certificates(roots);

    if !with_certificate { return Arc::new(builder.with_no_client_auth()); }

    let chain: Vec<CertificateDer<'static>> = CertificateDer::pem_file_iter(&material.client_cert).expect("client pem").map(|cert| cert.expect("client cert")).collect();
    let key = PrivateKeyDer::from_pem_file(&material.client_key).expect("client key");

    Arc::new(builder.with_client_auth_cert(chain, key).expect("client auth"))

}

#[test]
fn mutual_tls_verifies_clients_and_exposes_certificate_variables () {

    let origin = Origin::start();
    let material = material(&["localhost"]);
    let variables = [( "x-client-dn", "$ssl_client_s_dn" ), ( "x-client-issuer", "$ssl_client_i_dn" ), ( "x-client-verify", "$ssl_client_verify" ), ( "x-client-serial", "$ssl_client_serial" ), ( "x-client-fp", "$ssl_client_fingerprint" )];

    let running = proxy(origin.addr, |config| {

        secured(config, &material.cert, &material.key, 5_000);

        let tls = config.tls.as_mut().expect("tls");

        tls.client_ca = Some(material.ca.clone());
        tls.client_auth = ClientAuth::Required;
        config.request_headers = variables.iter().map(|( name, value )| ( name.to_string(), value.to_string() )).collect();

    });

    assert_eq!(handshake(running.addr(), &authenticated(&material, true)).0, 200);
    assert_eq!(handshake(running.addr(), &authenticated(&material, false)).0, 0, "a client without a certificate must be refused");

    let seen = origin.seen();

    assert_eq!(seen.len(), 1);
    assert!(seen[0].header("x-client-dn").is_some_and(|dn| dn.contains("CN=alice") && dn.contains("O=Acme")), "{:?}", seen[0].header("x-client-dn"));
    assert!(seen[0].header("x-client-issuer").is_some_and(|dn| dn.contains("aegisx test ca")), "{:?}", seen[0].header("x-client-issuer"));
    assert_eq!(seen[0].header("x-client-verify"), Some("SUCCESS"));
    assert!(seen[0].header("x-client-serial").is_some_and(|serial| !serial.is_empty() && serial.bytes().all(|byte| byte.is_ascii_hexdigit())), "{:?}", seen[0].header("x-client-serial"));
    assert!(seen[0].header("x-client-fp").is_some_and(|fp| fp.len() == 64), "{:?}", seen[0].header("x-client-fp"));

    running.stop().expect("stop");

    let running = proxy(origin.addr, |config| {

        secured(config, &material.cert, &material.key, 5_000);

        let tls = config.tls.as_mut().expect("tls");

        tls.client_ca = Some(material.ca.clone());
        tls.client_auth = ClientAuth::Optional;
        config.request_headers = variables.iter().map(|( name, value )| ( name.to_string(), value.to_string() )).collect();

    });

    assert_eq!(handshake(running.addr(), &authenticated(&material, false)).0, 200);
    assert_eq!(handshake(running.addr(), &authenticated(&material, true)).0, 200);

    let seen = origin.seen();

    assert_eq!(seen.len(), 2);
    assert_eq!(seen[0].header("x-client-verify"), Some("NONE"));
    assert_eq!(seen[0].header("x-client-dn"), Some(""));
    assert_eq!(seen[1].header("x-client-verify"), Some("SUCCESS"));

    running.stop().expect("stop");

    let invalid = Config::parse(r#"set_upstream("127.0.0.1:3000") set_tls { cert = "/dev/null", key = "/dev/null", client_auth = "required" }"#, "mtls.lua", std::path::Path::new("/tmp"));

    assert!(invalid.is_err());

}

#[test]
fn ocsp_staples_are_loaded_from_files () {

    let origin = Origin::start();
    let material = material(&["localhost"]);
    let staple = material.dir.join("staple.der");

    std::fs::write(&staple, [0x30, 0x03, 0x0a, 0x01, 0x00]).expect("staple");

    let running = proxy(origin.addr, |config| {

        secured(config, &material.cert, &material.key, 5_000);
        config.tls.as_mut().expect("tls").ocsp = Some(staple.clone());

    });

    assert_eq!(handshake(running.addr(), &client(&material.ca_pem)).0, 200);

    running.stop().expect("stop");

    let missing = Config { tls: Some(TlsConfig { cert: material.cert.clone(), key: material.key.clone(), ocsp: Some(material.dir.join("absent.der")), ..TlsConfig::default() }), ..Config::default() };

    assert!(Boot::check(&missing).is_err());

}


#[test]
fn backends_can_require_a_client_certificate () {

    let material = material(&["localhost"]);
    let origin = Origin::start_mtls(&material.cert, &material.key, &material.ca);

    let trusted = proxy(origin.addr, |config| {

        let backend = &mut config.pools.get_mut("default").expect("pool").backends[0];

        backend.tls = true;
        backend.server_name = "localhost".to_string();
        backend.ca_file = Some(material.ca.clone());
        backend.cert = Some(material.client_cert.clone());
        backend.key = Some(material.client_key.clone());

    });

    assert_eq!(Http1::connect(trusted.addr()).get("/with").status, 200);

    trusted.stop().expect("stop");

    let anonymous = proxy(origin.addr, |config| {

        let backend = &mut config.pools.get_mut("default").expect("pool").backends[0];

        backend.tls = true;
        backend.server_name = "localhost".to_string();
        backend.ca_file = Some(material.ca.clone());

    });

    assert_eq!(Http1::connect(anonymous.addr()).get("/without").status, 502);
    assert_eq!(origin.seen().len(), 1);

    anonymous.stop().expect("stop");

    let half = Config::parse(&format!(r#"add_upstream("default", {{ address = "127.0.0.1:3000", tls = true, server_name = "localhost", cert = "{}" }})"#, material.client_cert.display()), "mtls.lua", std::path::Path::new("/tmp")).map(|_| ()).map_err(|error| error.to_string());

    assert!(half.expect_err("cert without key").contains("key"));

}
