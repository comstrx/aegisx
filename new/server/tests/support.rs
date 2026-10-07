#![allow(dead_code)]

use std::convert::Infallible;
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll};
use std::thread;
use std::time::Duration;

use aegisx::app::{Boot, Running};
use aegisx::config::Config;
use bytes::Bytes;
use http_body::Frame;
use http_body_util::combinators::BoxBody;
use http_body_util::{BodyExt, Full};
use hyper::body::Incoming;
use hyper::service::service_fn;
use hyper_util::rt::{TokioExecutor, TokioIo};
use hyper_util::server::conn::auto;
use rcgen::{BasicConstraints, CertificateParams, CertifiedIssuer, DnType, ExtendedKeyUsagePurpose, IsCa, KeyPair};
use rustls::pki_types::pem::PemObject;
use rustls::pki_types::{CertificateDer, PrivateKeyDer, ServerName};
use rustls::{ClientConfig, ClientConnection, RootCertStore, ServerConfig, StreamOwned};
use tokio::net::TcpListener;
use tokio::sync::watch;
use tokio_rustls::TlsAcceptor;

pub type Headers = Vec<( String, String )>;

#[derive(Debug)]
pub struct Seen {
    pub method   : String,
    pub path     : String,
    pub version  : String,
    pub headers  : Headers,
    pub body     : Vec<u8>,
    pub trailers : Headers,
}

pub struct Origin {
    pub addr     : SocketAddr,
    pub accepted : Arc<AtomicUsize>,
    pub seen     : Arc<Mutex<Vec<Seen>>>,
    pub flaky    : Arc<AtomicUsize>,
    stop         : watch::Sender<bool>,
    handle       : Option<thread::JoinHandle<()>>,
}

struct Chunks {
    remaining : usize,
    size      : usize,
    delay     : Duration,
    sleep     : Option<Pin<Box<tokio::time::Sleep>>>,
}

pub struct Reply {
    pub status   : u16,
    pub headers  : Headers,
    pub body     : Vec<u8>,
    pub trailers : Headers,
}

struct Trailed {
    stage : u8,
}

impl http_body::Body for Trailed {

    type Data = Bytes;
    type Error = Infallible;

    fn poll_frame ( self: Pin<&mut Self>, _: &mut Context<'_> ) -> Poll<Option<Result<Frame<Bytes>, Infallible>>> {

        let this = self.get_mut();

        this.stage += 1;

        match this.stage {
            1 => Poll::Ready(Some(Ok(Frame::data(Bytes::from_static(b"trailed"))))),
            2 => {

                let mut map = http::HeaderMap::new();

                map.insert("x-checksum", http::HeaderValue::from_static("abc123"));

                Poll::Ready(Some(Ok(Frame::trailers(map))))

            }
            _ => Poll::Ready(None),
        }

    }

}

pub trait Transport: Read + Write {}

impl <T: Read + Write> Transport for T {}

pub struct Http1 {
    stream : Box<dyn Transport>,
    buffer : Vec<u8>,
    head   : bool,
}

pub struct Material {
    pub dir         : PathBuf,
    pub cert        : PathBuf,
    pub key         : PathBuf,
    pub ca          : PathBuf,
    pub ca_pem      : String,
    pub client_cert : PathBuf,
    pub client_key  : PathBuf,
}

impl http_body::Body for Chunks {

    type Data = Bytes;
    type Error = Infallible;

    fn poll_frame ( self: Pin<&mut Self>, cx: &mut Context<'_> ) -> Poll<Option<Result<Frame<Bytes>, Infallible>>> {

        let this = self.get_mut();

        if this.remaining == 0 { return Poll::Ready(None); }

        if let Some(sleep) = this.sleep.as_mut() {

            if sleep.as_mut().poll(cx).is_pending() { return Poll::Pending; }

            this.sleep = None;

        }

        this.remaining -= 1;
        this.sleep = Some(Box::pin(tokio::time::sleep(this.delay)));

        Poll::Ready(Some(Ok(Frame::data(Bytes::from(vec![b'c'; this.size])))))

    }

}

fn segment ( path: &str, index: usize ) -> usize {

    path.trim_start_matches('/').split('/').nth(index).and_then(|value| value.parse().ok()).unwrap_or(0)

}

async fn respond ( mut request: http::Request<Incoming>, accepted: Arc<AtomicUsize>, seen: Arc<Mutex<Vec<Seen>>>, flaky: Arc<AtomicUsize> ) -> Result<http::Response<BoxBody<Bytes, Infallible>>, Infallible> {

    let _ = accepted;

    let method = request.method().to_string();
    let path = request.uri().path_and_query().map(|value| value.to_string()).unwrap_or_default();
    let version = format!("{:?}", request.version());
    let mut headers: Vec<( String, String )> = request.headers().iter().map(|( name, value )| ( name.to_string(), String::from_utf8_lossy(value.as_bytes()).into_owned() )).collect();
    let route = path.split('?').next().unwrap_or("/").to_string();

    if !headers.iter().any(|( name, _ )| name == "host") && let Some(authority) = request.uri().authority() { headers.push(( "host".to_string(), authority.to_string() )); }

    if route == "/upgrade" {

        let pending = hyper::upgrade::on(&mut request);

        tokio::task::spawn_local(async move {

            use tokio::io::{AsyncReadExt, AsyncWriteExt};

            let Ok(upgraded) = pending.await else { return; };
            let mut io = TokioIo::new(upgraded);
            let mut buffer = [0u8; 1024];

            loop {

                let count = match io.read(&mut buffer).await { Ok(0) | Err(_) => break, Ok(count) => count };

                if io.write_all(&buffer[..count]).await.is_err() { break; }

            }

        });

        seen.lock().expect("seen").push(Seen { method, path, version, headers, body: Vec::new(), trailers: Vec::new() });

        let mut response = http::Response::new(http_body_util::Empty::<Bytes>::new().map_err(|never| match never {}).boxed());

        *response.status_mut() = http::StatusCode::SWITCHING_PROTOCOLS;
        response.headers_mut().insert("upgrade", http::HeaderValue::from_static("echo"));
        response.headers_mut().insert("connection", http::HeaderValue::from_static("upgrade"));

        return Ok(response);

    }

    let collected = request.into_body().collect().await.ok();
    let trailers = collected.as_ref().and_then(|collected| collected.trailers()).map(|map| map.iter().map(|( name, value )| ( name.to_string(), String::from_utf8_lossy(value.as_bytes()).into_owned() )).collect()).unwrap_or_default();
    let body = collected.map(|collected| collected.to_bytes().to_vec()).unwrap_or_default();

    let host = headers.iter().find(|( name, _ )| name == "host").map(|( _, value )| value.clone()).unwrap_or_default();
    let cookie = headers.iter().find(|( name, _ )| name == "cookie").map(|( _, value )| value.clone());
    let conditional = headers.iter().any(|( name, value )| name == "if-none-match" && value == "\"v1\"");

    seen.lock().expect("seen").push(Seen { method, path: path.clone(), version, headers, body: body.clone(), trailers });

    let mut response = http::Response::new(Full::new(Bytes::from_static(b"ok")).map_err(|never| match never {}).boxed());

    if flaky.load(Ordering::SeqCst) > 0 {

        flaky.fetch_sub(1, Ordering::SeqCst);
        *response.status_mut() = http::StatusCode::SERVICE_UNAVAILABLE;

        return Ok(response);

    }

    match route.as_str() {
        "/echo" => { *response.body_mut() = Full::new(Bytes::from(body)).map_err(|never| match never {}).boxed(); }
        "/redirect" => {

            *response.status_mut() = http::StatusCode::FOUND;
            response.headers_mut().insert("location", http::HeaderValue::from_str(&format!("http://{host}/landing?x=1")).expect("location"));
            response.headers_mut().insert("refresh", http::HeaderValue::from_str(&format!("0; url=http://{host}/again")).expect("refresh"));

        }
        "/etagged" => {

            response.headers_mut().insert("etag", http::HeaderValue::from_static("\"v1\""));
            response.headers_mut().insert("cache-control", http::HeaderValue::from_static("max-age=1"));
            response.headers_mut().insert("content-type", http::HeaderValue::from_static("text/plain"));

            if conditional {

                *response.status_mut() = http::StatusCode::NOT_MODIFIED;
                *response.body_mut() = http_body_util::Empty::<Bytes>::new().map_err(|never| match never {}).boxed();

            } else {

                *response.body_mut() = Full::new(Bytes::from_static(b"fresh body")).map_err(|never| match never {}).boxed();

            }

        }
        "/gzipped" => {

            use std::io::Write as _;

            let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());

            encoder.write_all(b"plain text that was gzipped by the origin").expect("gzip");

            let compressed = encoder.finish().expect("gzip finish");

            response.headers_mut().insert("content-encoding", http::HeaderValue::from_static("gzip"));
            response.headers_mut().insert("content-type", http::HeaderValue::from_static("text/plain"));
            response.headers_mut().insert("etag", http::HeaderValue::from_static("\"gz1\""));
            *response.body_mut() = Full::new(Bytes::from(compressed)).map_err(|never| match never {}).boxed();

        }
        "/verify" => {

            match cookie.as_deref() {
                Some("session=ok") => {

                    response.headers_mut().insert("x-user", http::HeaderValue::from_static("alice"));
                    response.headers_mut().insert("x-secret", http::HeaderValue::from_static("hidden"));

                }
                _ => {

                    *response.status_mut() = http::StatusCode::UNAUTHORIZED;
                    response.headers_mut().insert("www-authenticate", http::HeaderValue::from_static("Bearer realm=\"verify\""));
                    *response.body_mut() = Full::new(Bytes::from_static(b"who are you")).map_err(|never| match never {}).boxed();

                }
            }

        }
        "/cookie" => {

            response.headers_mut().append("set-cookie", http::HeaderValue::from_static("sid=1; Path=/app/; Domain=backend.local; HttpOnly"));
            response.headers_mut().append("set-cookie", http::HeaderValue::from_static("theme=dark; path=/other; domain=.Backend.Local"));

        }
        "/close" => { response.headers_mut().insert("connection", http::HeaderValue::from_static("close")); }
        "/trailers" => {

            response.headers_mut().insert("trailer", http::HeaderValue::from_static("x-checksum"));
            *response.body_mut() = Trailed { stage: 0 }.boxed();

        }
        "/hop" => {

            response.headers_mut().insert("connection", http::HeaderValue::from_static("x-bar, close"));
            response.headers_mut().insert("x-bar", http::HeaderValue::from_static("1"));
            response.headers_mut().insert("keep-alive", http::HeaderValue::from_static("timeout=5"));
            response.headers_mut().insert("x-keep", http::HeaderValue::from_static("yes"));

        }
        other if other.starts_with("/chunked/") => {

            let chunks = Chunks { remaining: segment(other, 1), size: segment(other, 2), delay: Duration::from_millis(segment(other, 3) as u64), sleep: None };
            *response.body_mut() = chunks.boxed();

        }
        other if other.starts_with("/slow/") => {

            tokio::time::sleep(Duration::from_millis(segment(other, 1) as u64)).await;

        }
        other if other.starts_with("/blockfor/") => {

            response.headers_mut().insert("x-block", http::HeaderValue::from(segment(other, 1)));

        }
        other if other.starts_with("/status/") => {

            *response.status_mut() = http::StatusCode::from_u16(segment(other, 1) as u16).unwrap_or(http::StatusCode::OK);

        }
        other if other.starts_with("/large/") => {

            *response.body_mut() = Full::new(Bytes::from(vec![b'x'; segment(other, 1)])).map_err(|never| match never {}).boxed();

        }
        _ => {}
    }

    Ok(response)

}

impl Origin {

    pub fn start () -> Self {

        Self::launch(None)

    }

    pub fn start_tls ( cert: &Path, key: &Path ) -> Self {

        let chain: Vec<CertificateDer<'static>> = CertificateDer::pem_file_iter(cert).expect("cert pem").map(|cert| cert.expect("cert")).collect();
        let key = PrivateKeyDer::from_pem_file(key).expect("key pem");
        let mut config = ServerConfig::builder().with_no_client_auth().with_single_cert(chain, key).expect("origin tls");

        config.alpn_protocols = vec![b"h2".to_vec(), b"http/1.1".to_vec()];

        Self::launch(Some(Arc::new(config)))

    }

    pub fn start_mtls ( cert: &Path, key: &Path, ca: &Path ) -> Self {

        let chain: Vec<CertificateDer<'static>> = CertificateDer::pem_file_iter(cert).expect("cert pem").map(|cert| cert.expect("cert")).collect();
        let key = PrivateKeyDer::from_pem_file(key).expect("key pem");
        let mut roots = RootCertStore::empty();

        for cert in CertificateDer::pem_file_iter(ca).expect("ca pem") { roots.add(cert.expect("ca cert")).expect("root"); }

        let verifier = rustls::server::WebPkiClientVerifier::builder(Arc::new(roots)).build().expect("client verifier");
        let mut config = ServerConfig::builder().with_client_cert_verifier(verifier).with_single_cert(chain, key).expect("origin tls");

        config.alpn_protocols = vec![b"h2".to_vec(), b"http/1.1".to_vec()];

        Self::launch(Some(Arc::new(config)))

    }

    #[cfg(unix)]
    pub fn start_unix ( path: &std::path::Path ) -> Self {

        let _ = std::fs::remove_file(path);

        let accepted = Arc::new(AtomicUsize::new(0));
        let seen = Arc::new(Mutex::new(Vec::new()));
        let flaky = Arc::new(AtomicUsize::new(0));
        let ( stop, mut stopped ) = watch::channel(false);
        let ( ready, bound ) = std::sync::mpsc::channel();
        let path = path.to_path_buf();

        let handle = thread::spawn({

            let accepted = accepted.clone();
            let seen = seen.clone();
            let flaky = flaky.clone();

            move || {

                let runtime = tokio::runtime::Builder::new_current_thread().enable_all().build().expect("origin runtime");
                let local = tokio::task::LocalSet::new();

                local.block_on(&runtime, async move {

                    let listener = tokio::net::UnixListener::bind(&path).expect("origin unix bind");

                    ready.send(()).expect("origin ready");

                    loop {

                        let ( stream, _ ) = tokio::select! {
                            accepted = listener.accept() => accepted.expect("origin accept"),
                            _ = stopped.wait_for(|value| *value) => break,
                        };

                        accepted.fetch_add(1, Ordering::SeqCst);

                        let accepted = accepted.clone();
                        let seen = seen.clone();
                        let flaky = flaky.clone();

                        tokio::task::spawn_local(async move {

                            let service = service_fn(move |request| respond(request, accepted.clone(), seen.clone(), flaky.clone()));
                            let builder = auto::Builder::new(TokioExecutor::new());

                            let _ = builder.serve_connection_with_upgrades(TokioIo::new(stream), service).await;

                        });

                    }

                });

            }

        });

        bound.recv_timeout(Duration::from_secs(5)).expect("origin started");

        Self { addr: SocketAddr::from(( [127, 0, 0, 1], 0 )), accepted, seen, flaky, stop, handle: Some(handle) }

    }

    fn launch ( tls: Option<Arc<ServerConfig>> ) -> Self {

        let accepted = Arc::new(AtomicUsize::new(0));
        let seen = Arc::new(Mutex::new(Vec::new()));
        let flaky = Arc::new(AtomicUsize::new(0));
        let ( stop, mut stopped ) = watch::channel(false);
        let ( ready, bound ) = std::sync::mpsc::channel();

        let handle = thread::spawn({

            let accepted = accepted.clone();
            let seen = seen.clone();
            let flaky = flaky.clone();

            move || {

                let runtime = tokio::runtime::Builder::new_current_thread().enable_all().build().expect("origin runtime");
                let local = tokio::task::LocalSet::new();

                local.block_on(&runtime, async move {

                    let listener = TcpListener::bind("127.0.0.1:0").await.expect("origin bind");
                    ready.send(listener.local_addr().expect("origin addr")).expect("origin ready");

                    loop {

                        let ( stream, _ ) = tokio::select! {
                            accepted = listener.accept() => accepted.expect("origin accept"),
                            _ = stopped.wait_for(|value| *value) => break,
                        };

                        accepted.fetch_add(1, Ordering::SeqCst);

                        let accepted = accepted.clone();
                        let seen = seen.clone();
                        let flaky = flaky.clone();
                        let acceptor = tls.clone().map(TlsAcceptor::from);

                        tokio::task::spawn_local(async move {

                            let service = service_fn(move |request| respond(request, accepted.clone(), seen.clone(), flaky.clone()));
                            let builder = auto::Builder::new(TokioExecutor::new());

                            match acceptor {
                                Some(acceptor) => {

                                    if let Ok(stream) = acceptor.accept(stream).await { let _ = builder.serve_connection_with_upgrades(TokioIo::new(stream), service).await; }

                                }
                                None => { let _ = builder.serve_connection_with_upgrades(TokioIo::new(stream), service).await; }
                            }

                        });

                    }

                });

            }

        });

        let addr = bound.recv_timeout(Duration::from_secs(5)).expect("origin started");

        Self { addr, accepted, seen, flaky, stop, handle: Some(handle) }

    }

    pub fn authority ( &self ) -> String {

        self.addr.to_string()

    }

    pub fn accepted ( &self ) -> usize {

        self.accepted.load(Ordering::SeqCst)

    }

    pub fn flaky ( &self, count: usize ) {

        self.flaky.store(count, Ordering::SeqCst);

    }

    pub fn seen ( &self ) -> Vec<Seen> {

        self.seen.lock().expect("seen").drain(..).collect()

    }

}

impl Drop for Origin {

    fn drop ( &mut self ) {

        let _ = self.stop.send(true);

        if let Some(handle) = self.handle.take() { let _ = handle.join(); }

    }

}

impl Seen {

    pub fn header ( &self, name: &str ) -> Option<&str> {

        self.headers.iter().find(|( key, _ )| key.eq_ignore_ascii_case(name)).map(|( _, value )| value.as_str())
    }

}

pub fn free_port () -> SocketAddr {

    let probe = std::net::TcpListener::bind("127.0.0.1:0").expect("probe bind");

    probe.local_addr().expect("probe addr")

}

static PORTS: Mutex<()> = Mutex::new(());

pub fn proxy ( upstream: SocketAddr, tune: impl FnOnce(&mut Config) ) -> Running {

    let guard = PORTS.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    let mut config = Config { listen: free_port(), ..Config::default() };

    config.set_upstream(upstream);
    config.runtime.workers = 2;
    config.runtime.pin = false;

    tune(&mut config);

    let running = Boot::start(config).expect("proxy start");

    drop(guard);

    wait_for(running.addr());

    running

}

pub fn wait_for ( addr: SocketAddr ) {

    for _ in 0..100 {

        if TcpStream::connect_timeout(&addr, Duration::from_millis(100)).is_ok() { return; }

        thread::sleep(Duration::from_millis(20));

    }

    panic!("{addr} never became reachable");

}

impl Http1 {

    pub fn connect ( addr: SocketAddr ) -> Self {

        Self { stream: Box::new(Self::tcp(addr)), buffer: Vec::new(), head: false }

    }

    pub fn connect_tls ( addr: SocketAddr, ca_pem: &str, name: &str ) -> Self {

        let mut roots = RootCertStore::empty();

        for cert in CertificateDer::pem_slice_iter(ca_pem.as_bytes()) { roots.add(cert.expect("ca cert")).expect("root"); }

        let config = Arc::new(ClientConfig::builder().with_root_certificates(roots).with_no_client_auth());
        let name = ServerName::try_from(name.to_string()).expect("server name");
        let session = ClientConnection::new(config, name).expect("client session");

        Self { stream: Box::new(StreamOwned::new(session, Self::tcp(addr))), buffer: Vec::new(), head: false }

    }

    fn tcp ( addr: SocketAddr ) -> TcpStream {

        let stream = TcpStream::connect(addr).expect("client connect");

        stream.set_read_timeout(Some(Duration::from_secs(15))).expect("read timeout");
        stream.set_nodelay(true).expect("nodelay");

        stream

    }

    pub fn send ( &mut self, raw: &[u8] ) {

        self.stream.write_all(raw).expect("client write");

    }

    pub fn try_send ( &mut self, raw: &[u8] ) -> bool {

        self.stream.write_all(raw).and_then(|_| self.stream.flush()).is_ok()

    }

    pub fn request ( &mut self, method: &str, path: &str, headers: &[( &str, &str )], body: &[u8] ) -> Reply {

        let custom_host = headers.iter().any(|( name, _ )| name.eq_ignore_ascii_case("host"));
        let mut raw = format!("{method} {path} HTTP/1.1\r\nContent-Length: {}\r\n", body.len());

        if !custom_host { raw.push_str("Host: test.local\r\n"); }

        for ( name, value ) in headers { raw.push_str(&format!("{name}: {value}\r\n")); }

        raw.push_str("\r\n");

        self.head = method.eq_ignore_ascii_case("HEAD");
        self.send(raw.as_bytes());
        self.send(body);

        self.reply()

    }

    pub fn get ( &mut self, path: &str ) -> Reply {

        self.request("GET", path, &[], b"")

    }

    pub fn reply ( &mut self ) -> Reply {

        self.try_reply().expect("reply")

    }

    pub fn try_reply ( &mut self ) -> Option<Reply> {

        let head = loop {

            if let Some(at) = self.buffer.windows(4).position(|window| window == b"\r\n\r\n") { break at; }

            if !self.fill()? { return None; }

        };

        let header_text = String::from_utf8_lossy(&self.buffer[..head]).into_owned();
        self.buffer.drain(..head + 4);

        let mut lines = header_text.lines();
        let status = lines.next()?.split_whitespace().nth(1)?.parse().ok()?;
        let headers: Vec<( String, String )> = lines.filter_map(|line| line.split_once(':')).map(|( name, value )| ( name.trim().to_ascii_lowercase(), value.trim().to_string() )).collect();

        let chunked = headers.iter().any(|( name, value )| name == "transfer-encoding" && value.contains("chunked"));
        let length = headers.iter().find(|( name, _ )| name == "content-length").and_then(|( _, value )| value.parse::<usize>().ok());
        let bodiless = std::mem::take(&mut self.head) || status < 200 || status == 204 || status == 304;
        let mut trailers = Vec::new();

        let body = if bodiless { Vec::new() } else if chunked { let ( body, found ) = self.read_chunked()?; trailers = found; body } else if let Some(length) = length { self.read_exact(length)? } else { self.read_to_end() };

        Some(Reply { status, headers, body, trailers })

    }

    fn fill ( &mut self ) -> Option<bool> {

        let mut chunk = [0u8; 65536];

        match self.stream.read(&mut chunk) {
            Ok(0) => Some(false),
            Ok(count) => { self.buffer.extend_from_slice(&chunk[..count]); Some(true) }
            Err(_) => None,
        }

    }

    fn read_exact ( &mut self, length: usize ) -> Option<Vec<u8>> {

        while self.buffer.len() < length {

            if !self.fill()? { return None; }

        }

        Some(self.buffer.drain(..length).collect())

    }

    fn read_to_end ( &mut self ) -> Vec<u8> {

        while let Some(true) = self.fill() {}

        std::mem::take(&mut self.buffer)

    }

    pub fn raw ( &mut self, bytes: &[u8], expect: usize ) -> Vec<u8> {

        self.send(bytes);

        while self.buffer.len() < expect {

            if !self.fill().unwrap_or(false) { break; }

        }

        let take = self.buffer.len().min(expect);

        self.buffer.drain(..take).collect()

    }

    fn read_chunked ( &mut self ) -> Option<( Vec<u8>, Headers )> {

        let mut body = Vec::new();

        loop {

            let line_end = loop {

                if let Some(at) = self.buffer.windows(2).position(|window| window == b"\r\n") { break at; }

                if !self.fill()? { return None; }

            };

            let size_text = String::from_utf8_lossy(&self.buffer[..line_end]).into_owned();
            self.buffer.drain(..line_end + 2);

            let size = usize::from_str_radix(size_text.split(';').next()?.trim(), 16).ok()?;

            if size == 0 {

                let mut trailers = Vec::new();

                loop {

                    let end = loop {

                        if let Some(at) = self.buffer.windows(2).position(|window| window == b"\r\n") { break at; }

                        if !self.fill()? { return None; }

                    };

                    let line = String::from_utf8_lossy(&self.buffer[..end]).into_owned();

                    self.buffer.drain(..end + 2);

                    if line.is_empty() { break; }

                    if let Some(( name, value )) = line.split_once(':') { trailers.push(( name.trim().to_ascii_lowercase(), value.trim().to_string() )); }

                }

                return Some(( body, trailers ));

            }

            body.extend(self.read_exact(size)?);
            self.read_exact(2)?;

        }

    }

}

impl Reply {

    pub fn header ( &self, name: &str ) -> Option<&str> {

        self.headers.iter().find(|( key, _ )| key == name).map(|( _, value )| value.as_str())

    }

    pub fn text ( &self ) -> String {

        String::from_utf8_lossy(&self.body).into_owned()

    }

}

static NONCE: AtomicUsize = AtomicUsize::new(0);

pub fn material ( names: &[&str] ) -> Material {

    let ca_key = KeyPair::generate().expect("ca key");
    let mut ca_params = CertificateParams::new(Vec::<String>::new()).expect("ca params");

    ca_params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
    ca_params.distinguished_name.push(DnType::CommonName, "aegisx test ca");

    let ca = CertifiedIssuer::self_signed(ca_params, ca_key).expect("ca");
    let key = KeyPair::generate().expect("leaf key");
    let params = CertificateParams::new(names.iter().map(|name| name.to_string()).collect::<Vec<_>>()).expect("leaf params");
    let cert = params.signed_by(&key, &*ca).expect("leaf");

    let dir = std::env::temp_dir().join(format!("aegisx-test-{}-{}", std::process::id(), NONCE.fetch_add(1, Ordering::SeqCst)));

    std::fs::create_dir_all(&dir).expect("material dir");

    let paths = ( dir.join("cert.pem"), dir.join("key.pem"), dir.join("ca.pem") );
    let ca_pem = ca.pem();

    std::fs::write(&paths.0, cert.pem()).expect("write cert");
    std::fs::write(&paths.1, key.serialize_pem()).expect("write key");
    std::fs::write(&paths.2, &ca_pem).expect("write ca");

    let client_key = KeyPair::generate().expect("client key");
    let mut client_params = CertificateParams::new(Vec::<String>::new()).expect("client params");

    client_params.distinguished_name.push(DnType::CommonName, "alice");
    client_params.distinguished_name.push(DnType::OrganizationName, "Acme");
    client_params.extended_key_usages = vec![ExtendedKeyUsagePurpose::ClientAuth];

    let client = client_params.signed_by(&client_key, &*ca).expect("client leaf");
    let client_paths = ( dir.join("client_cert.pem"), dir.join("client_key.pem") );

    std::fs::write(&client_paths.0, client.pem()).expect("write client cert");
    std::fs::write(&client_paths.1, client_key.serialize_pem()).expect("write client key");

    Material { dir, cert: paths.0, key: paths.1, ca: paths.2, ca_pem, client_cert: client_paths.0, client_key: client_paths.1 }

}

impl Drop for Material {

    fn drop ( &mut self ) {

        let _ = std::fs::remove_dir_all(&self.dir);

    }

}
