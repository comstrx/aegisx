mod support;

use std::fs;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use aegisx::config::Route;
use flate2::read::GzDecoder;
use support::{Http1, Origin, proxy};

fn gunzip ( body: &[u8] ) -> Vec<u8> {

    let mut out = Vec::new();

    GzDecoder::new(body).read_to_end(&mut out).expect("gzip stream");

    out

}

fn unbrotli ( body: &[u8] ) -> Vec<u8> {

    let mut out = Vec::new();

    brotli::Decompressor::new(body, 4096).read_to_end(&mut out).expect("brotli stream");

    out

}

fn site () -> PathBuf {

    let dir = std::env::temp_dir().join(format!("aegisx-compress-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);

    fs::create_dir_all(&dir).expect("site dir");
    fs::write(dir.join("app.js"), (0..65_536u32).map(|index| b"const x = 1;\n"[index as usize % 13]).collect::<Vec<u8>>()).expect("app.js");
    fs::write(dir.join("tiny.css"), "a{}").expect("tiny.css");
    fs::write(dir.join("blob.bin"), vec![7u8; 8192]).expect("blob.bin");

    dir

}

#[test]
fn proxied_responses_are_gzipped_when_the_client_accepts_it () {

    let origin = Origin::start();
    let running = proxy(origin.addr, |config| { config.compression.enabled = true; config.response_headers.insert("content-type".to_string(), "text/plain".to_string()); });
    let mut client = Http1::connect(running.addr());
    let body: Vec<u8> = (0..20_000u32).map(|index| b"hello world "[index as usize % 12]).collect();

    let plain = client.request("POST", "/echo", &[( "Content-Type", "text/plain" )], &body);

    assert_eq!(plain.status, 200);
    assert_eq!(plain.header("content-encoding"), None);
    assert_eq!(plain.body, body);

    let gzipped = client.request("POST", "/echo", &[( "Content-Type", "text/plain" ), ( "Accept-Encoding", "gzip, deflate" )], &body);

    assert_eq!(gzipped.status, 200);
    assert_eq!(gzipped.header("content-encoding"), Some("gzip"));
    assert_eq!(gzipped.header("vary"), Some("Accept-Encoding"));
    assert_eq!(gzipped.header("content-length"), None);
    assert_eq!(gzipped.header("transfer-encoding"), Some("chunked"));
    assert!(gzipped.body.len() < body.len() / 4, "compressed {} bytes", gzipped.body.len());
    assert_eq!(gunzip(&gzipped.body), body);

    let brotlied = client.request("POST", "/echo", &[( "Content-Type", "text/plain; charset=utf-8" ), ( "Accept-Encoding", "gzip;q=0.8, br;q=1.0" )], &body);

    assert_eq!(brotlied.header("content-encoding"), Some("br"));
    assert_eq!(unbrotli(&brotlied.body), body);

    let wildcard = client.request("POST", "/echo", &[( "Content-Type", "text/plain" ), ( "Accept-Encoding", "*" )], &body);

    assert_eq!(wildcard.header("content-encoding"), Some("zstd"));
    assert_eq!(zstd::decode_all(&wildcard.body[..]).expect("zstd"), body);

    let refused = client.request("POST", "/echo", &[( "Content-Type", "text/plain" ), ( "Accept-Encoding", "gzip;q=0, br;q=0" )], &body);

    assert_eq!(refused.header("content-encoding"), None);
    assert_eq!(refused.body, body);

    running.stop().expect("stop");

}

#[test]
fn small_binary_and_already_encoded_bodies_pass_through () {

    let origin = Origin::start();
    let running = proxy(origin.addr, |config| {

        config.compression.enabled = true;
        config.routes.push(Route { name: "bin".to_string(), path: "/bin".to_string(), upstream: "default".to_string(), strip_prefix: true, response_headers: [( "content-type".to_string(), "application/octet-stream".to_string() )].into(), ..Route::default() });
        config.routes.push(Route { name: "untyped".to_string(), path: "/untyped".to_string(), upstream: "default".to_string(), strip_prefix: true, ..Route::default() });
        config.routes.push(Route { name: "text".to_string(), path: "/".to_string(), upstream: "default".to_string(), response_headers: [( "content-type".to_string(), "text/plain".to_string() )].into(), ..Route::default() });

    });
    let mut client = Http1::connect(running.addr());
    let body: Vec<u8> = vec![b'x'; 4096];

    let small = client.request("POST", "/echo", &[( "Accept-Encoding", "gzip" )], b"short");

    assert_eq!(small.header("content-encoding"), None);
    assert_eq!(small.text(), "short");

    let binary = client.request("POST", "/bin/echo", &[( "Accept-Encoding", "gzip" )], &body);

    assert_eq!(binary.header("content-encoding"), None);
    assert_eq!(binary.header("content-length"), Some("4096"));

    let untyped = client.request("POST", "/untyped/echo", &[( "Accept-Encoding", "gzip" )], &body);

    assert_eq!(untyped.header("content-encoding"), None);
    assert_eq!(untyped.body, body);

    let head = client.request("HEAD", "/echo", &[( "Accept-Encoding", "gzip" )], b"");

    assert_eq!(head.status, 200);

    running.stop().expect("stop");

}

#[test]
fn chunked_upstreams_stream_compressed_frames_without_waiting_for_the_end () {

    let origin = Origin::start();
    let running = proxy(origin.addr, |config| { config.compression.enabled = true; config.response_headers.insert("content-type".to_string(), "text/plain".to_string()); });
    let mut client = Http1::connect(running.addr());

    let whole = client.request("GET", "/chunked/4/4096/10", &[( "Accept-Encoding", "gzip" )], b"");

    assert_eq!(whole.status, 200);
    assert_eq!(whole.header("content-encoding"), Some("gzip"));
    assert_eq!(gunzip(&whole.body), vec![b'c'; 4 * 4096]);

    let mut stream = TcpStream::connect(running.addr()).expect("connect");

    stream.set_read_timeout(Some(Duration::from_millis(400))).expect("timeout");
    stream.write_all(b"GET /chunked/3/2048/700 HTTP/1.1\r\nHost: test.local\r\nAccept-Encoding: gzip\r\n\r\n").expect("send");

    let started = Instant::now();
    let mut seen = Vec::new();
    let mut chunk = [0u8; 8192];

    while started.elapsed() < Duration::from_millis(1_000) {

        match stream.read(&mut chunk) {
            Ok(0) => break,
            Ok(count) => seen.extend_from_slice(&chunk[..count]),
            Err(_) => {}
        }

        if let Some(at) = seen.windows(4).position(|window| window == b"\r\n\r\n") && seen.len() > at + 4 + 20 { break; }

    }

    let head = seen.windows(4).position(|window| window == b"\r\n\r\n").expect("response head");

    assert!(seen.len() > head + 4 + 20, "no compressed body bytes within {:?}: {} bytes total", started.elapsed(), seen.len());
    assert!(started.elapsed() < Duration::from_millis(1_400), "first frame only arrived after {:?}", started.elapsed());

    running.stop().expect("stop");

}

#[test]
fn static_files_compress_and_weaken_their_etag () {

    let origin = Origin::start();
    let root = site();
    let running = proxy(origin.addr, |config| {
        config.compression.enabled = true;
        config.routes.push(Route { name: "files".to_string(), path: "/".to_string(), root: Some(root.clone()), ..Route::default() });
    });
    let mut client = Http1::connect(running.addr());

    let plain = client.get("/app.js");
    let etag = plain.header("etag").expect("etag").to_string();

    assert_eq!(plain.header("content-length"), Some("65536"));

    let gzipped = client.request("GET", "/app.js", &[( "Accept-Encoding", "gzip" )], b"");

    assert_eq!(gzipped.status, 200);
    assert_eq!(gzipped.header("content-encoding"), Some("gzip"));
    assert_eq!(gzipped.header("etag").map(str::to_string), Some(format!("W/{etag}")));
    assert_eq!(gzipped.header("accept-ranges"), None);
    assert_eq!(gzipped.header("content-type"), Some("text/javascript"));
    assert_eq!(gunzip(&gzipped.body).len(), 65_536);

    let cached = client.request("GET", "/app.js", &[( "Accept-Encoding", "gzip" ), ( "If-None-Match", &etag )], b"");

    assert_eq!(cached.status, 304);
    assert_eq!(cached.header("content-encoding"), None);

    let ranged = client.request("GET", "/app.js", &[( "Accept-Encoding", "gzip" ), ( "Range", "bytes=0-99" )], b"");

    assert_eq!(ranged.status, 206);
    assert_eq!(ranged.header("content-encoding"), None);
    assert_eq!(ranged.body.len(), 100);

    let tiny = client.request("GET", "/tiny.css", &[( "Accept-Encoding", "gzip" )], b"");

    assert_eq!(tiny.header("content-encoding"), None);

    let blob = client.request("GET", "/blob.bin", &[( "Accept-Encoding", "gzip" )], b"");

    assert_eq!(blob.header("content-encoding"), None);

    running.stop().expect("stop");

}

#[test]
fn routes_can_opt_out_and_in () {

    let origin = Origin::start();
    let running = proxy(origin.addr, |config| {
        config.response_headers.insert("content-type".to_string(), "text/plain".to_string());
        config.routes.push(Route { name: "raw".to_string(), path: "/raw".to_string(), upstream: "default".to_string(), strip_prefix: true, compress: Some(false), ..Route::default() });
        config.routes.push(Route { name: "packed".to_string(), path: "/".to_string(), upstream: "default".to_string(), compress: Some(true), ..Route::default() });
    });
    let mut client = Http1::connect(running.addr());
    let body: Vec<u8> = vec![b'y'; 4096];

    let raw = client.request("POST", "/raw/echo", &[( "Accept-Encoding", "gzip" )], &body);

    assert_eq!(raw.header("content-encoding"), None);
    assert_eq!(raw.body, body);

    let packed = client.request("POST", "/echo", &[( "Accept-Encoding", "gzip" )], &body);

    assert_eq!(packed.header("content-encoding"), Some("gzip"));
    assert_eq!(gunzip(&packed.body), body);

    running.stop().expect("stop");

}

#[test]
fn zstd_is_preferred_when_offered_and_can_be_switched_off () {

    let origin = Origin::start();
    let running = proxy(origin.addr, |config| { config.compression.enabled = true; config.response_headers.insert("content-type".to_string(), "text/plain".to_string()); });
    let mut client = Http1::connect(running.addr());
    let body: Vec<u8> = (0..20_000u32).map(|index| b"hello world "[index as usize % 12]).collect();

    let zstded = client.request("POST", "/echo", &[( "Content-Type", "text/plain" ), ( "Accept-Encoding", "gzip, br, zstd" )], &body);

    assert_eq!(zstded.status, 200);
    assert_eq!(zstded.header("content-encoding"), Some("zstd"));
    assert_eq!(zstded.header("vary"), Some("Accept-Encoding"));
    assert!(zstded.body.len() < body.len() / 4, "compressed {} bytes", zstded.body.len());
    assert_eq!(zstd::decode_all(&zstded.body[..]).expect("zstd"), body);

    let brotlied = client.request("POST", "/echo", &[( "Content-Type", "text/plain" ), ( "Accept-Encoding", "br, zstd;q=0" )], &body);

    assert_eq!(brotlied.header("content-encoding"), Some("br"));

    running.stop().expect("stop");

    let running = proxy(origin.addr, |config| { config.compression.enabled = true; config.compression.zstd = false; config.response_headers.insert("content-type".to_string(), "text/plain".to_string()); });
    let mut client = Http1::connect(running.addr());

    let fallback = client.request("POST", "/echo", &[( "Content-Type", "text/plain" ), ( "Accept-Encoding", "zstd, gzip" )], &body);

    assert_eq!(fallback.header("content-encoding"), Some("gzip"));
    assert_eq!(gunzip(&fallback.body), body);

    running.stop().expect("stop");

    assert!(aegisx::config::Config::parse(r#"set_upstream("127.0.0.1:3000") set_compression { enabled = true, zstd_level = 25 }"#, "z.lua", std::path::Path::new("/tmp")).is_err());

}

#[test]
fn gunzip_decodes_upstream_gzip_for_clients_that_cannot_take_it () {

    let origin = Origin::start();
    let running = proxy(origin.addr, |config| {
        config.routes.push(Route { name: "plain".to_string(), path: "/plain".to_string(), upstream: "default".to_string(), strip_prefix: true, gunzip: Some(true), ..Route::default() });
        config.routes.push(Route { name: "raw".to_string(), path: "/".to_string(), upstream: "default".to_string(), ..Route::default() });
    });
    let mut client = Http1::connect(running.addr());

    let decoded = client.get("/plain/gzipped");

    assert_eq!(decoded.status, 200);
    assert_eq!(decoded.header("content-encoding"), None);
    assert_eq!(decoded.header("etag"), Some("W/\"gz1\""));
    assert_eq!(decoded.text(), "plain text that was gzipped by the origin");

    let kept = client.request("GET", "/plain/gzipped", &[( "Accept-Encoding", "gzip" )], b"");

    assert_eq!(kept.header("content-encoding"), Some("gzip"));
    assert_eq!(gunzip(&kept.body), b"plain text that was gzipped by the origin");

    let untouched = client.get("/gzipped");

    assert_eq!(untouched.header("content-encoding"), Some("gzip"));

    running.stop().expect("stop");

}

