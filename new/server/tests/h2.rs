mod support;

use std::sync::Arc;

use aegisx::config::{Config, TlsConfig};
use bytes::Bytes;
use http::{HeaderMap, Request};
use rustls::pki_types::pem::PemObject;
use rustls::pki_types::{CertificateDer, ServerName};
use rustls::{ClientConfig, RootCertStore};
use support::{Origin, proxy};
use tokio::net::TcpStream;
use tokio_rustls::TlsConnector;

async fn read_all ( body: &mut h2::RecvStream ) -> ( Vec<u8>, Option<HeaderMap> ) {

    let mut data = Vec::new();

    while let Some(chunk) = body.data().await {

        let chunk = chunk.expect("h2 data");

        body.flow_control().release_capacity(chunk.len()).expect("release");
        data.extend_from_slice(&chunk);

    }

    ( data, body.trailers().await.expect("h2 trailers") )

}

#[tokio::test]
async fn cleartext_http2_requests_are_served_from_the_same_listener () {

    let origin = Origin::start();
    let running = proxy(origin.addr, |config: &mut Config| {

        config.server.h2c = true;
        config.routes.push(aegisx::config::Route { name: "all".to_string(), path: "/".to_string(), upstream: "default".to_string(), ..Default::default() });

    });
    let addr = running.addr();

    let tcp = TcpStream::connect(addr).await.expect("connect");
    let ( client, connection ) = h2::client::handshake(tcp).await.expect("h2 handshake");

    tokio::spawn(async move { let _ = connection.await; });

    let mut client = client.ready().await.expect("ready");
    let request = Request::builder().method("GET").uri(format!("http://{addr}/over-h2")).body(()).expect("request");
    let ( response, _ ) = client.send_request(request, true).expect("send");
    let response = response.await.expect("response");

    assert_eq!(response.status(), 200);
    assert!(response.headers().get("x-request-id").is_some());

    let ( parts, mut body ) = response.into_parts();
    let ( data, _ ) = read_all(&mut body).await;

    assert_eq!(data, b"ok");
    assert_eq!(parts.version, http::Version::HTTP_2);

    let mut client = client.ready().await.expect("ready");
    let request = Request::builder().method("POST").uri(format!("http://{addr}/echo")).header("te", "trailers").header("trailer", "x-sig").body(()).expect("request");
    let ( response, mut stream ) = client.send_request(request, false).expect("send");

    stream.send_data(Bytes::from_static(b"hello over h2"), false).expect("data");

    let mut trailers = HeaderMap::new();

    trailers.insert("x-sig", http::HeaderValue::from_static("sig"));
    stream.send_trailers(trailers).expect("trailers");

    let response = response.await.expect("response");

    assert_eq!(response.status(), 200);

    let ( _, mut body ) = response.into_parts();
    let ( data, _ ) = read_all(&mut body).await;

    assert_eq!(data, b"hello over h2");

    let mut client = client.ready().await.expect("ready");
    let request = Request::builder().method("GET").uri(format!("http://{addr}/trailers")).header("te", "trailers").body(()).expect("request");
    let ( response, _ ) = client.send_request(request, true).expect("send");
    let response = response.await.expect("response");
    let ( _, mut body ) = response.into_parts();
    let ( data, trailers ) = read_all(&mut body).await;

    assert_eq!(data, b"trailed");
    assert_eq!(trailers.and_then(|map| map.get("x-checksum").cloned()).as_ref().and_then(|value| value.to_str().ok()), Some("abc123"));

    let seen = origin.seen();

    assert_eq!(seen.len(), 3);
    assert_eq!(seen[0].path, "/over-h2");
    assert_eq!(seen[1].body, b"hello over h2");
    assert_eq!(seen[1].trailers, vec![( "x-sig".to_string(), "sig".to_string() )]);
    assert_eq!(seen[1].header("x-forwarded-proto"), Some("http"));

    running.stop().expect("stop");

}

#[tokio::test]
async fn tls_negotiates_http2_through_alpn () {

    let origin = Origin::start();
    let material = support::material(&["localhost"]);
    let running = proxy(origin.addr, |config: &mut Config| {

        config.tls = Some(TlsConfig { cert: material.cert.clone(), key: material.key.clone(), ..TlsConfig::default() });
        config.routes.push(aegisx::config::Route { name: "vhost".to_string(), host: Some("localhost".to_string()), path: "/".to_string(), upstream: "default".to_string(), request_headers: std::collections::BTreeMap::from([( "x-vhost".to_string(), "matched".to_string() )]), ..aegisx::config::Route::default() });
        config.routes.push(aegisx::config::Route { name: "all".to_string(), path: "/".to_string(), upstream: "default".to_string(), ..aegisx::config::Route::default() });

    });
    let addr = running.addr();

    let mut roots = RootCertStore::empty();

    for cert in CertificateDer::pem_slice_iter(material.ca_pem.as_bytes()) { roots.add(cert.expect("ca")).expect("root"); }

    let mut config = ClientConfig::builder().with_root_certificates(roots).with_no_client_auth();

    config.alpn_protocols = vec![b"h2".to_vec(), b"http/1.1".to_vec()];

    let connector = TlsConnector::from(Arc::new(config));
    let tcp = TcpStream::connect(addr).await.expect("connect");
    let tls = connector.connect(ServerName::try_from("localhost").expect("name"), tcp).await.expect("tls");

    assert_eq!(tls.get_ref().1.alpn_protocol(), Some(b"h2".as_slice()));

    let ( client, connection ) = h2::client::handshake(tls).await.expect("h2 handshake");

    tokio::spawn(async move { let _ = connection.await; });

    let mut client = client.ready().await.expect("ready");
    let request = Request::builder().method("GET").uri("https://localhost/secure-h2").body(()).expect("request");
    let ( response, _ ) = client.send_request(request, true).expect("send");
    let response = response.await.expect("response");

    assert_eq!(response.status(), 200);

    let ( _, mut body ) = response.into_parts();
    let ( data, _ ) = read_all(&mut body).await;

    let seen = origin.seen();

    assert_eq!(data, b"ok");
    assert_eq!(seen[0].header("x-forwarded-proto"), Some("https"));
    assert_eq!(seen[0].header("x-vhost"), Some("matched"));

    running.stop().expect("stop");

}

#[tokio::test]
async fn http2_backends_carry_requests_and_trailers_over_one_connection () {

    let origin = Origin::start();

    let running = proxy(origin.addr, |config: &mut Config| {

        config.pools.get_mut("default").expect("pool").backends[0].protocol = aegisx::http::upstream::Protocol::Http2;
        config.routes.push(aegisx::config::Route { name: "all".to_string(), path: "/".to_string(), upstream: "default".to_string(), ..Default::default() });

    });

    let addr = running.addr();

    let mut client = support::Http1::connect(addr);

    assert_eq!(client.get("/first").status, 200);
    assert_eq!(client.get("/second").status, 200);

    let reply = client.request("POST", "/echo", &[( "TE", "trailers" )], b"over h2 upstream");

    assert_eq!(reply.status, 200);
    assert_eq!(reply.text(), "over h2 upstream");

    let reply = client.request("GET", "/trailers", &[( "TE", "trailers" )], b"");

    assert_eq!(reply.text(), "trailed");
    assert_eq!(reply.trailers, vec![( "x-checksum".to_string(), "abc123".to_string() )]);

    let seen = origin.seen();

    assert!(seen.iter().all(|seen| seen.version == "HTTP/2.0"), "{:?}", seen.iter().map(|seen| seen.version.clone()).collect::<Vec<_>>());
    assert_eq!(origin.accepted(), 1);

    running.stop().expect("stop");

}

#[tokio::test]
async fn tls_backends_pick_http2_through_alpn_unless_pinned_to_http1 () {

    let material = support::material(&["localhost"]);
    let origin = Origin::start_tls(&material.cert, &material.key);

    let secure = |config: &mut Config, protocol: aegisx::http::upstream::Protocol| {

        let backend = &mut config.pools.get_mut("default").expect("pool").backends[0];

        backend.tls = true;
        backend.server_name = "localhost".to_string();
        backend.ca_file = Some(material.ca.clone());
        backend.protocol = protocol;

    };

    let negotiated = proxy(origin.addr, |config| secure(config, aegisx::http::upstream::Protocol::Auto));

    assert_eq!(support::Http1::connect(negotiated.addr()).get("/auto").status, 200);
    assert_eq!(origin.seen()[0].version, "HTTP/2.0");

    negotiated.stop().expect("stop");

    let pinned = proxy(origin.addr, |config| secure(config, aegisx::http::upstream::Protocol::Http1));

    assert_eq!(support::Http1::connect(pinned.addr()).get("/pinned").status, 200);
    assert_eq!(origin.seen()[0].version, "HTTP/1.1");

    pinned.stop().expect("stop");

}

#[tokio::test]
async fn websockets_ride_http2_through_extended_connect () {

    let origin = Origin::start();
    let running = proxy(origin.addr, |config: &mut Config| config.server.h2c = true);
    let addr = running.addr();
    let tcp = TcpStream::connect(addr).await.expect("connect");
    let ( client, connection ) = h2::client::handshake(tcp).await.expect("h2 handshake");

    tokio::spawn(async move { let _ = connection.await; });

    let mut client = client.ready().await.expect("ready");

    for _ in 0..100 {

        if client.is_extended_connect_protocol_enabled() { break; }

        tokio::time::sleep(std::time::Duration::from_millis(10)).await;

    }

    assert!(client.is_extended_connect_protocol_enabled(), "the server offers the extended CONNECT protocol");

    let tunnel = |path: &str| {

        let mut request = Request::builder().method("CONNECT").uri(format!("http://{addr}{path}")).header("sec-websocket-version", "13").body(()).expect("request");

        request.extensions_mut().insert(h2::ext::Protocol::from_static("websocket"));

        request

    };

    let ( response, mut stream ) = client.send_request(tunnel("/upgrade"), false).expect("send");
    let response = response.await.expect("response");

    assert_eq!(response.status(), 200);
    assert!(response.headers().get("connection").is_none() && response.headers().get("upgrade").is_none() && response.headers().get("sec-websocket-accept").is_none());

    let mut body = response.into_body();

    stream.send_data(Bytes::from_static(b"ping over h2"), false).expect("data");

    let chunk = body.data().await.expect("echo").expect("echo data");

    assert_eq!(&chunk[..], b"ping over h2");

    stream.send_data(Bytes::new(), true).expect("close");

    let seen = origin.seen();

    assert_eq!(seen[0].method, "GET");
    assert_eq!(seen[0].header("upgrade"), Some("websocket"));
    assert_eq!(seen[0].header("sec-websocket-version"), Some("13"));
    assert!(seen[0].header("sec-websocket-key").is_some_and(|key| key.len() == 24));

    let mut client = client.ready().await.expect("ready");
    let ( refused, _stream ) = client.send_request(tunnel("/plain"), false).expect("send");

    assert_eq!(refused.await.expect("response").status(), 502, "an upstream that does not switch protocols opens no tunnel");

    running.stop().expect("stop");

}

