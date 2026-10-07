mod support;

use std::sync::Arc;

use aegisx::config::TlsConfig;
use bytes::Buf;
use http::Request;
use quinn::crypto::rustls::QuicClientConfig;
use rustls::RootCertStore;
use rustls::pki_types::{CertificateDer, pem::PemObject};
use support::{Origin, material, proxy};

#[tokio::test(flavor = "current_thread")]
async fn http3_requests_reach_the_backend_over_quic () {

    let material = material(&["localhost"]);
    let origin = Origin::start();
    let running = proxy(origin.addr, |config| {

        config.tls = Some(TlsConfig { cert: material.cert.clone(), key: material.key.clone(), ..TlsConfig::default() });
        config.http3.enabled = true;

    });
    let addr = running.addr();

    let mut roots = RootCertStore::empty();

    for cert in CertificateDer::pem_slice_iter(material.ca_pem.as_bytes()) { roots.add(cert.expect("ca cert")).expect("root"); }

    let mut tls = rustls::ClientConfig::builder().with_root_certificates(roots).with_no_client_auth();

    tls.alpn_protocols = vec![b"h3".to_vec()];

    let mut endpoint = quinn::Endpoint::client("127.0.0.1:0".parse().expect("bind")).expect("client endpoint");

    endpoint.set_default_client_config(quinn::ClientConfig::new(Arc::new(QuicClientConfig::try_from(tls).expect("quic client config"))));

    let connection = endpoint.connect(addr, "localhost").expect("connect").await.expect("quic handshake");
    let ( mut driver, mut sender ) = h3::client::new(h3_quinn::Connection::new(connection)).await.expect("h3 client");

    let driver_task = tokio::spawn(async move { let _ = std::future::poll_fn(|cx| driver.poll_close(cx)).await; });

    let request = Request::get(format!("https://localhost:{}/h3/path?x=1", addr.port())).header("x-probe", "quic").body(()).expect("request");
    let mut stream = sender.send_request(request).await.expect("send request");

    stream.finish().await.expect("finish");

    let response = stream.recv_response().await.expect("response");

    assert_eq!(response.status(), 200);
    assert!(response.headers().get("alt-svc").is_some_and(|value| value.to_str().unwrap_or("").starts_with("h3=")));

    let mut body = Vec::new();

    while let Some(mut chunk) = stream.recv_data().await.expect("data") { body.extend_from_slice(&chunk.copy_to_bytes(chunk.remaining())); }

    assert_eq!(body, b"ok");

    let seen = origin.seen();

    assert_eq!(seen[0].path, "/h3/path?x=1");
    assert_eq!(seen[0].header("x-probe"), Some("quic"));
    assert_eq!(seen[0].header("x-forwarded-proto"), Some("https"));
    assert_eq!(seen[0].header("transfer-encoding"), None);

    drop(sender);
    endpoint.close(0u32.into(), b"done");
    let _ = driver_task.await;

    running.stop().expect("stop");

}

#[test]
fn http3_requires_tls () {

    let mut config = aegisx::config::Config { listen: support::free_port(), ..aegisx::config::Config::default() };

    config.set_upstream(std::net::SocketAddr::from(( [127, 0, 0, 1], 1 )));
    config.http3.enabled = true;

    let error = config.validate().expect_err("http3 without tls").to_string();

    assert!(error.contains("set_tls"), "{error}");

}
