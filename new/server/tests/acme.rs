mod support;

use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::Path;
use std::sync::Arc;

use aegisx::config::{AcmeChallenge, AcmeConfig, Config, TlsConfig};
use aegisx::http::Tls;
use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use rustls::{ClientConfig, ClientConnection, DigitallySignedStruct, SignatureScheme, StreamOwned};
use support::{Http1, Origin, free_port, proxy};

#[derive(Debug)]
struct Trusting;

impl ServerCertVerifier for Trusting {

    fn verify_server_cert ( &self, _: &CertificateDer<'_>, _: &[CertificateDer<'_>], _: &ServerName<'_>, _: &[u8], _: UnixTime ) -> Result<ServerCertVerified, rustls::Error> { Ok(ServerCertVerified::assertion()) }

    fn verify_tls12_signature ( &self, _: &[u8], _: &CertificateDer<'_>, _: &DigitallySignedStruct ) -> Result<HandshakeSignatureValid, rustls::Error> { Ok(HandshakeSignatureValid::assertion()) }

    fn verify_tls13_signature ( &self, _: &[u8], _: &CertificateDer<'_>, _: &DigitallySignedStruct ) -> Result<HandshakeSignatureValid, rustls::Error> { Ok(HandshakeSignatureValid::assertion()) }

    fn supported_verify_schemes ( &self ) -> Vec<SignatureScheme> { vec![SignatureScheme::ECDSA_NISTP256_SHA256, SignatureScheme::ECDSA_NISTP384_SHA384, SignatureScheme::ED25519, SignatureScheme::RSA_PSS_SHA256] }

}

fn trusting ( alpn: Option<&[u8]> ) -> Arc<ClientConfig> {

    let mut config = ClientConfig::builder().dangerous().with_custom_certificate_verifier(Arc::new(Trusting)).with_no_client_auth();

    if let Some(protocol) = alpn { config.alpn_protocols = vec![protocol.to_vec()]; }

    Arc::new(config)

}

fn peer_certificate ( addr: std::net::SocketAddr, name: &str, config: &Arc<ClientConfig>, send: bool ) -> Option<Vec<u8>> {

    let name = ServerName::try_from(name.to_string()).expect("server name");
    let session = ClientConnection::new(config.clone(), name).expect("client session");
    let mut stream = StreamOwned::new(session, TcpStream::connect(addr).expect("connect"));

    if send {

        stream.write_all(b"GET / HTTP/1.1\r\nHost: acme.test\r\nConnection: close\r\n\r\n").expect("write");

        let mut raw = Vec::new();
        let _ = stream.read_to_end(&mut raw);

    } else {

        let _ = stream.flush();
        let _ = stream.conn.complete_io(&mut stream.sock);

    }

    stream.conn.peer_certificates().and_then(|chain| chain.first()).map(|cert| cert.as_ref().to_vec())

}

fn acme ( dir: &Path, challenge: AcmeChallenge, listen: Option<std::net::SocketAddr> ) -> TlsConfig {

    TlsConfig { acme: Some(AcmeConfig { domains: vec!["acme.test".to_string(), "www.acme.test".to_string()], directory: format!("https://127.0.0.1:{}/directory", free_port().port()), cache_dir: dir.to_path_buf(), challenge, listen, ..AcmeConfig::default() }), ..TlsConfig::default() }

}

#[test]
fn acme_starts_with_a_placeholder_and_answers_tls_alpn_challenges () {

    let origin = Origin::start();
    let dir = std::env::temp_dir().join(format!("aegisx-acme-{}-alpn", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);

    let running = proxy(origin.addr, |config| config.tls = Some(acme(&dir, AcmeChallenge::TlsAlpn01, None)));

    assert!(dir.join("cert.pem").is_file() && dir.join("key.pem").is_file(), "placeholder material missing");

    let placeholder = peer_certificate(running.addr(), "acme.test", &trusting(None), true).expect("placeholder certificate");
    let ( _, parsed ) = x509_parser::parse_x509_certificate(&placeholder).expect("parse placeholder");

    assert!(parsed.subject().to_string().contains("AegisX placeholder"), "{}", parsed.subject());
    assert!(parsed.subject_alternative_name().ok().flatten().is_some(), "placeholder has no SAN");

    let digest = [7u8; 32];
    let challenges = running.state().challenges().expect("challenge map");

    challenges.write().expect("map").insert("acme.test".to_string(), Tls::challenge("acme.test", &digest).expect("challenge cert"));

    let answered = peer_certificate(running.addr(), "acme.test", &trusting(Some(b"acme-tls/1")), false).expect("challenge certificate");
    let ( _, parsed ) = x509_parser::parse_x509_certificate(&answered).expect("parse challenge");
    let identifier = parsed.extensions().iter().find(|extension| extension.oid.to_id_string() == "1.3.6.1.5.5.7.1.31").expect("acme identifier extension");

    assert!(identifier.critical);
    assert!(identifier.value.ends_with(&digest), "{:?}", identifier.value);
    assert!(peer_certificate(running.addr(), "other.test", &trusting(Some(b"acme-tls/1")), false).is_none(), "unknown names must not get a challenge certificate");

    let regular = peer_certificate(running.addr(), "acme.test", &trusting(None), true).expect("regular certificate");

    assert_eq!(regular, placeholder);

    running.stop().expect("stop");

    let _ = std::fs::remove_dir_all(&dir);

}

#[test]
fn acme_http_listener_serves_tokens_and_redirects_everything_else () {

    let origin = Origin::start();
    let dir = std::env::temp_dir().join(format!("aegisx-acme-{}-http", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let listen = free_port();

    let running = proxy(origin.addr, |config| config.tls = Some(acme(&dir, AcmeChallenge::Http01, Some(listen))));

    support::wait_for(listen);

    running.state().tokens().expect("token map").write().expect("map").insert("tok3n".to_string(), "tok3n.thumbprint".to_string());

    let mut client = Http1::connect(listen);
    let served = client.request("GET", "/.well-known/acme-challenge/tok3n", &[( "Host", "acme.test" )], b"");

    assert_eq!(served.status, 200);
    assert_eq!(served.header("content-type"), Some("text/plain"));
    assert_eq!(served.text(), "tok3n.thumbprint");

    let missing = Http1::connect(listen).request("GET", "/.well-known/acme-challenge/nope", &[( "Host", "acme.test" )], b"");

    assert_eq!(missing.status, 308);
    assert_eq!(missing.header("location"), Some("https://acme.test/.well-known/acme-challenge/nope"));

    let redirected = Http1::connect(listen).request("GET", "/app?x=1", &[( "Host", "acme.test:80" )], b"");

    assert_eq!(redirected.status, 308);
    assert_eq!(redirected.header("location"), Some("https://acme.test/app?x=1"));

    running.stop().expect("stop");

    let _ = std::fs::remove_dir_all(&dir);

}

#[test]
fn acme_configuration_is_validated () {

    let source = |acme: &str| format!(r#"
        set_upstream("127.0.0.1:3000")
        set_tls {{ acme = {{ {acme} }} }}
    "#);

    assert!(Config::parse(&source(r#"domains = { "a.test" }, cache_dir = "/tmp/aegisx-acme-ok""#), "ok.lua", Path::new("/tmp")).is_ok());
    assert!(Config::parse(&source(r#"cache_dir = "/tmp/x""#), "domains.lua", Path::new("/tmp")).is_err());
    assert!(Config::parse(&source(r#"domains = { "*.a.test" }, cache_dir = "/tmp/x""#), "wild.lua", Path::new("/tmp")).is_err());
    assert!(Config::parse(&source(r#"domains = { "a.test" }, directory = "http://insecure", cache_dir = "/tmp/x""#), "dir.lua", Path::new("/tmp")).is_err());
    assert!(Config::parse(&source(r#"domains = { "a.test" }, renew_days = 0, cache_dir = "/tmp/x""#), "renew.lua", Path::new("/tmp")).is_err());
    assert!(Config::parse(r#"set_upstream("127.0.0.1:3000") set_tls { cert = "/dev/null" }"#, "key.lua", Path::new("/tmp")).is_err());

    let config = Config::parse(&source(r#"domains = { "a.test" }, directory = "staging", challenge = "http_01", listen = "127.0.0.1:8080", email = "ops@a.test""#), "full.lua", Path::new("/tmp")).expect("config");
    let acme = config.tls.expect("tls").acme.expect("acme");

    assert_eq!(acme.challenge, AcmeChallenge::Http01);
    assert_eq!(acme.email.as_deref(), Some("ops@a.test"));
    assert_eq!(acme.renew_days, 30);

}
