mod support;

use std::collections::HashMap;
use std::convert::Infallible;
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use aegisx::config::{AcmeChallenge, AcmeConfig, TlsConfig};
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use bytes::Bytes;
use http_body_util::combinators::BoxBody;
use http_body_util::{BodyExt, Full};
use hyper::body::Incoming;
use hyper::service::service_fn;
use hyper_util::rt::{TokioExecutor, TokioIo};
use hyper_util::server::conn::auto;
use rcgen::{BasicConstraints, CertificateParams, CertificateSigningRequestParams, CertifiedIssuer, DnType, IsCa, KeyPair};
use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::pki_types::pem::PemObject;
use rustls::pki_types::{CertificateDer, CertificateSigningRequestDer, PrivateKeyDer, ServerName, UnixTime};
use rustls::{ClientConfig, ClientConnection, DigitallySignedStruct, RootCertStore, ServerConfig, SignatureScheme, StreamOwned};
use serde_json::{Value, json};
use support::{Http1, Origin, proxy};
use tokio::net::TcpListener;
use tokio::sync::watch;
use tokio_rustls::TlsAcceptor;

#[derive(Debug)]
struct Trusting;

impl ServerCertVerifier for Trusting {

    fn verify_server_cert ( &self, _: &CertificateDer<'_>, _: &[CertificateDer<'_>], _: &ServerName<'_>, _: &[u8], _: UnixTime ) -> Result<ServerCertVerified, rustls::Error> { Ok(ServerCertVerified::assertion()) }

    fn verify_tls12_signature ( &self, _: &[u8], _: &CertificateDer<'_>, _: &DigitallySignedStruct ) -> Result<HandshakeSignatureValid, rustls::Error> { Ok(HandshakeSignatureValid::assertion()) }

    fn verify_tls13_signature ( &self, _: &[u8], _: &CertificateDer<'_>, _: &DigitallySignedStruct ) -> Result<HandshakeSignatureValid, rustls::Error> { Ok(HandshakeSignatureValid::assertion()) }

    fn supported_verify_schemes ( &self ) -> Vec<SignatureScheme> { vec![SignatureScheme::ECDSA_NISTP256_SHA256, SignatureScheme::ECDSA_NISTP384_SHA384, SignatureScheme::ED25519, SignatureScheme::RSA_PSS_SHA256] }

}

struct Fake {
    thumbprint : Option<String>,
    valid      : HashMap<usize, bool>,
    failures   : Vec<String>,
    issued     : Option<String>,
    proxy      : Option<SocketAddr>,
    nonces     : u64,
}

struct Directory {
    addr   : SocketAddr,
    ca     : PathBuf,
    state  : Arc<Mutex<Fake>>,
    stop   : watch::Sender<bool>,
    handle : Option<thread::JoinHandle<()>>,
}

const DOMAINS: [&str; 2] = ["acme.test", "www.acme.test"];

fn peer_certificate ( addr: SocketAddr, name: &str, alpn: &[u8] ) -> Option<Vec<u8>> {

    let mut config = ClientConfig::builder().dangerous().with_custom_certificate_verifier(Arc::new(Trusting)).with_no_client_auth();

    config.alpn_protocols = vec![alpn.to_vec()];

    let session = ClientConnection::new(Arc::new(config), ServerName::try_from(name.to_string()).ok()?).ok()?;
    let mut stream = StreamOwned::new(session, TcpStream::connect(addr).ok()?);

    let _ = stream.flush();
    let _ = stream.conn.complete_io(&mut stream.sock);

    stream.conn.peer_certificates().and_then(|chain| chain.first()).map(|cert| cert.as_ref().to_vec())

}

fn validate ( state: &Arc<Mutex<Fake>>, index: usize ) -> Result<(), String> {

    let ( proxy, thumbprint ) = { let fake = state.lock().expect("fake"); ( fake.proxy, fake.thumbprint.clone() ) };
    let proxy = proxy.ok_or("proxy address unknown")?;
    let thumbprint = thumbprint.ok_or("no account thumbprint")?;
    let name = DOMAINS[index - 1];
    let der = peer_certificate(proxy, name, b"acme-tls/1").ok_or("no challenge certificate served")?;
    let ( _, parsed ) = x509_parser::parse_x509_certificate(&der).map_err(|error| error.to_string())?;
    let extension = parsed.extensions().iter().find(|extension| extension.oid.to_id_string() == "1.3.6.1.5.5.7.1.31").ok_or("acme identifier extension missing")?;
    let expected = aws_lc_rs::digest::digest(&aws_lc_rs::digest::SHA256, format!("tok{index}.{thumbprint}").as_bytes());

    if !extension.critical { return Err("acme identifier extension is not critical".to_string()); }

    if !extension.value.ends_with(expected.as_ref()) { return Err("acme identifier digest mismatch".to_string()); }

    let names: Vec<String> = parsed.subject_alternative_name().ok().flatten().map(|san| san.value.general_names.iter().map(|name| name.to_string()).collect()).unwrap_or_default();

    if !names.iter().any(|san| san.contains(name)) { return Err(format!("challenge certificate lacks SAN {name}: {names:?}")); }

    Ok(())

}

fn thumbprint ( protected: &Value ) -> Option<String> {

    let jwk = protected.get("jwk")?;
    let canonical = format!("{{\"crv\":{},\"kty\":{},\"x\":{},\"y\":{}}}", jwk.get("crv")?, jwk.get("kty")?, jwk.get("x")?, jwk.get("y")?);

    Some(URL_SAFE_NO_PAD.encode(aws_lc_rs::digest::digest(&aws_lc_rs::digest::SHA256, canonical.as_bytes())))

}

fn body ( status: u16, nonce: u64, kind: &'static str, text: String, location: Option<String> ) -> http::Response<BoxBody<Bytes, Infallible>> {

    let mut response = http::Response::new(Full::new(Bytes::from(text)).map_err(|never| match never {}).boxed());

    *response.status_mut() = http::StatusCode::from_u16(status).expect("status");
    response.headers_mut().insert("replay-nonce", http::HeaderValue::from_str(&format!("nonce-{nonce}")).expect("nonce"));
    response.headers_mut().insert("content-type", http::HeaderValue::from_static(kind));

    if let Some(location) = location { response.headers_mut().insert("location", http::HeaderValue::from_str(&location).expect("location")); }

    response

}

async fn respond ( request: http::Request<Incoming>, base: String, state: Arc<Mutex<Fake>>, issuer: Arc<CertifiedIssuer<'static, KeyPair>>, ca_pem: String ) -> Result<http::Response<BoxBody<Bytes, Infallible>>, Infallible> {

    let path = request.uri().path().to_string();
    let raw = request.into_body().collect().await.map(|collected| collected.to_bytes()).unwrap_or_default();
    let jws: Value = serde_json::from_slice(&raw).unwrap_or(Value::Null);
    let protected: Value = jws.get("protected").and_then(Value::as_str).and_then(|text| URL_SAFE_NO_PAD.decode(text).ok()).and_then(|bytes| serde_json::from_slice(&bytes).ok()).unwrap_or(Value::Null);
    let payload: Value = jws.get("payload").and_then(Value::as_str).and_then(|text| URL_SAFE_NO_PAD.decode(text).ok()).and_then(|bytes| serde_json::from_slice(&bytes).ok()).unwrap_or(Value::Null);
    let nonce = { let mut fake = state.lock().expect("fake"); fake.nonces += 1; fake.nonces };
    let order = |fake: &Fake| {

        let status = if fake.issued.is_some() { "valid" } else if DOMAINS.iter().enumerate().all(|( index, _ )| fake.valid.get(&(index + 1)).copied().unwrap_or(false)) { "ready" } else { "pending" };

        json!({ "status": status, "identifiers": DOMAINS.iter().map(|name| json!({ "type": "dns", "value": name })).collect::<Vec<_>>(), "authorizations": [format!("{base}/authz/1"), format!("{base}/authz/2")], "finalize": format!("{base}/order/1/finalize"), "certificate": fake.issued.as_ref().map(|_| format!("{base}/cert/1")) })

    };

    Ok(match path.as_str() {
        "/directory" => body(200, nonce, "application/json", json!({ "newNonce": format!("{base}/nonce"), "newAccount": format!("{base}/account"), "newOrder": format!("{base}/order"), "revokeCert": format!("{base}/revoke"), "keyChange": format!("{base}/key-change") }).to_string(), None),
        "/nonce" => body(200, nonce, "application/json", String::new(), None),
        "/account" => {

            state.lock().expect("fake").thumbprint = thumbprint(&protected);

            body(201, nonce, "application/json", json!({ "status": "valid", "contact": payload.get("contact").cloned().unwrap_or(json!([])) }).to_string(), Some(format!("{base}/account/1")))

        }
        "/order" => { let fake = state.lock().expect("fake"); body(201, nonce, "application/json", order(&fake).to_string(), Some(format!("{base}/order/1"))) }
        "/order/1" => { let fake = state.lock().expect("fake"); body(200, nonce, "application/json", order(&fake).to_string(), None) }
        "/authz/1" | "/authz/2" => {

            let index: usize = path.rsplit('/').next().and_then(|digit| digit.parse().ok()).unwrap_or(1);
            let valid = state.lock().expect("fake").valid.get(&index).copied().unwrap_or(false);
            let status = if valid { "valid" } else { "pending" };

            body(200, nonce, "application/json", json!({ "identifier": { "type": "dns", "value": DOMAINS[index - 1] }, "status": status, "challenges": [{ "type": "tls-alpn-01", "url": format!("{base}/chall/{index}"), "token": format!("tok{index}"), "status": status }, { "type": "http-01", "url": format!("{base}/chall-http/{index}"), "token": format!("tok{index}"), "status": "pending" }] }).to_string(), None)

        }
        "/chall/1" | "/chall/2" => {

            let index: usize = path.rsplit('/').next().and_then(|digit| digit.parse().ok()).unwrap_or(1);

            match tokio::task::spawn_blocking({ let state = state.clone(); move || validate(&state, index) }).await.unwrap_or_else(|_| Err("validator panicked".to_string())) {
                Ok(()) => { state.lock().expect("fake").valid.insert(index, true); body(200, nonce, "application/json", json!({ "type": "tls-alpn-01", "url": format!("{base}/chall/{index}"), "token": format!("tok{index}"), "status": "valid" }).to_string(), None) }
                Err(reason) => { state.lock().expect("fake").failures.push(reason.clone()); body(400, nonce, "application/problem+json", json!({ "type": "urn:ietf:params:acme:error:unauthorized", "detail": reason, "status": 400 }).to_string(), None) }
            }

        }
        "/order/1/finalize" => {

            let csr = payload.get("csr").and_then(Value::as_str).and_then(|text| URL_SAFE_NO_PAD.decode(text).ok()).unwrap_or_default();
            let params = CertificateSigningRequestParams::from_der(&CertificateSigningRequestDer::from(csr)).expect("csr");
            let leaf = params.signed_by(&issuer).expect("sign leaf");
            let mut fake = state.lock().expect("fake");

            fake.issued = Some(format!("{}{}", leaf.pem(), ca_pem));

            body(200, nonce, "application/json", order(&fake).to_string(), None)

        }
        "/cert/1" => { let pem = state.lock().expect("fake").issued.clone().unwrap_or_default(); body(200, nonce, "application/pem-certificate-chain", pem, None) }
        _ => body(404, nonce, "application/problem+json", json!({ "type": "urn:ietf:params:acme:error:malformed", "detail": format!("unknown {path}"), "status": 404 }).to_string(), None),
    })

}

impl Directory {

    fn start () -> Self {

        let ca_key = KeyPair::generate().expect("ca key");
        let mut ca_params = CertificateParams::new(Vec::<String>::new()).expect("ca params");

        ca_params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
        ca_params.distinguished_name.push(DnType::CommonName, "fake acme ca");

        let issuer = Arc::new(CertifiedIssuer::self_signed(ca_params, ca_key).expect("ca"));
        let ca_pem = issuer.pem();
        let api_key = KeyPair::generate().expect("api key");
        let api_cert = CertificateParams::new(vec!["127.0.0.1".to_string(), "localhost".to_string()]).expect("api params").signed_by(&api_key, &*issuer).expect("api cert");
        let dir = std::env::temp_dir().join(format!("aegisx-fake-acme-{}", std::process::id()));

        std::fs::create_dir_all(&dir).expect("dir");

        let ca = dir.join("ca.pem");

        std::fs::write(&ca, &ca_pem).expect("write ca");

        let chain = vec![api_cert.der().clone()];
        let key = PrivateKeyDer::Pkcs8(api_key.serialize_der().into());
        let tls = Arc::new(ServerConfig::builder().with_no_client_auth().with_single_cert(chain, key).expect("api tls"));
        let state = Arc::new(Mutex::new(Fake { thumbprint: None, valid: HashMap::new(), failures: Vec::new(), issued: None, proxy: None, nonces: 0 }));
        let ( stop, mut stopped ) = watch::channel(false);
        let ( ready, bound ) = std::sync::mpsc::channel();

        let handle = thread::spawn({

            let state = state.clone();

            move || {

                let runtime = tokio::runtime::Builder::new_current_thread().enable_all().build().expect("fake runtime");
                let local = tokio::task::LocalSet::new();

                local.block_on(&runtime, async move {

                    let listener = TcpListener::bind("127.0.0.1:0").await.expect("fake bind");
                    let addr = listener.local_addr().expect("fake addr");
                    let base = format!("https://{addr}");

                    ready.send(addr).expect("fake ready");

                    loop {

                        let ( stream, _ ) = tokio::select! {
                            accepted = listener.accept() => accepted.expect("fake accept"),
                            _ = stopped.wait_for(|value| *value) => break,
                        };

                        let acceptor = TlsAcceptor::from(tls.clone());
                        let ( base, state, issuer, ca_pem ) = ( base.clone(), state.clone(), issuer.clone(), ca_pem.clone() );

                        tokio::task::spawn_local(async move {

                            let service = service_fn(move |request| respond(request, base.clone(), state.clone(), issuer.clone(), ca_pem.clone()));

                            if let Ok(stream) = acceptor.accept(stream).await { let _ = auto::Builder::new(TokioExecutor::new()).serve_connection(TokioIo::new(stream), service).await; }

                        });

                    }

                });

            }

        });

        let addr = bound.recv_timeout(Duration::from_secs(5)).expect("fake started");

        Self { addr, ca, state, stop, handle: Some(handle) }

    }

    fn directory ( &self ) -> String {

        format!("https://{}/directory", self.addr)

    }

}

impl Drop for Directory {

    fn drop ( &mut self ) {

        let _ = self.stop.send(true);

        if let Some(handle) = self.handle.take() { let _ = handle.join(); }

    }

}

#[test]
fn acme_orders_a_certificate_through_tls_alpn_01_and_installs_it () {

    let origin = Origin::start();
    let fake = Directory::start();
    let cache = std::env::temp_dir().join(format!("aegisx-acme-flow-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&cache);

    let running = proxy(origin.addr, |config| {
        config.tls = Some(TlsConfig { acme: Some(AcmeConfig { domains: DOMAINS.iter().map(|name| name.to_string()).collect(), email: Some("ops@acme.test".to_string()), directory: fake.directory(), directory_ca: Some(fake.ca.clone()), cache_dir: cache.clone(), challenge: AcmeChallenge::TlsAlpn01, listen: None, renew_days: 30, ..AcmeConfig::default() }), ..TlsConfig::default() });
    });

    fake.state.lock().expect("fake").proxy = Some(running.addr());

    let mut issued = false;

    for _ in 0..300 {

        if let Some(der) = peer_certificate(running.addr(), "acme.test", b"http/1.1") && let Ok(( _, parsed )) = x509_parser::parse_x509_certificate(&der) && parsed.issuer().to_string().contains("fake acme ca") { issued = true; break; }

        thread::sleep(Duration::from_millis(100));

    }

    let failures = fake.state.lock().expect("fake").failures.clone();

    assert!(failures.is_empty(), "challenge validation failed: {failures:?}");
    assert!(issued, "the proxy never switched to the issued certificate");

    let mut roots = RootCertStore::empty();

    for cert in CertificateDer::pem_file_iter(&fake.ca).expect("ca pem") { roots.add(cert.expect("ca cert")).expect("root"); }

    let verified = Arc::new(ClientConfig::builder().with_root_certificates(roots).with_no_client_auth());
    let session = ClientConnection::new(verified, ServerName::try_from("www.acme.test".to_string()).expect("name")).expect("session");
    let mut stream = StreamOwned::new(session, TcpStream::connect(running.addr()).expect("connect"));

    stream.write_all(b"GET /issued HTTP/1.1\r\nHost: www.acme.test\r\nConnection: close\r\n\r\n").expect("write");

    let mut raw = Vec::new();
    let _ = stream.read_to_end(&mut raw);
    let text = String::from_utf8_lossy(&raw);

    assert!(text.starts_with("HTTP/1.1 200"), "{text}");
    assert_eq!(origin.seen().len(), 1);
    assert!(cache.join("account.json").is_file());
    assert!(std::fs::read_to_string(cache.join("cert.pem")).expect("cert").contains("BEGIN CERTIFICATE"));
    assert!(running.state().challenges().expect("map").read().expect("map").is_empty(), "challenge certificates must be removed after validation");

    running.stop().expect("stop");
    drop(Http1::connect(origin.addr));

    let _ = std::fs::remove_dir_all(&cache);

}
