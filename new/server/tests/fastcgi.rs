mod support;

use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use aegisx::config::{BackendConfig, PoolConfig, Route};
use aegisx::http::upstream::Protocol;
use support::{Http1, Origin, proxy};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Mood {
    Keep,
    Close,
    Vanish,
    Noisy,
}

struct Php {
    addr     : SocketAddr,
    accepted : Arc<AtomicUsize>,
}

fn record ( out: &mut Vec<u8>, kind: u8, content: &[u8] ) {

    out.extend_from_slice(&[1, kind, 0, 1]);
    out.extend_from_slice(&(content.len() as u16).to_be_bytes());
    out.extend_from_slice(&[0, 0]);
    out.extend_from_slice(content);

}

fn pairs ( params: &[u8] ) -> BTreeMap<String, String> {

    let mut pairs = BTreeMap::new();
    let mut at = 0;

    while at < params.len() {

        let size = |at: &mut usize| { let first = params[*at]; if first < 128 { *at += 1; usize::from(first) } else { let wide = u32::from_be_bytes([params[*at] & 0x7f, params[*at + 1], params[*at + 2], params[*at + 3]]); *at += 4; wide as usize } };
        let ( name, value ) = ( size(&mut at), size(&mut at) );

        pairs.insert(String::from_utf8_lossy(&params[at..at + name]).into_owned(), String::from_utf8_lossy(&params[at + name..at + name + value]).into_owned());
        at += name + value;

    }

    pairs

}

fn serve ( mut stream: TcpStream, mood: Mood, serial: usize ) {

    let mut served = 0usize;

    loop {

        let ( mut params, mut body, mut begun ) = ( Vec::new(), Vec::new(), false );

        loop {

            let mut header = [0u8; 8];

            if stream.read_exact(&mut header).is_err() { return; }

            let length = usize::from(u16::from_be_bytes([header[4], header[5]]));
            let mut content = vec![0u8; length + usize::from(header[6])];

            if stream.read_exact(&mut content).is_err() { return; }

            content.truncate(length);

            match ( header[1], content.is_empty() ) {
                ( 1, _ ) => begun = true,
                ( 4, false ) => params.extend_from_slice(&content),
                ( 5, false ) => body.extend_from_slice(&content),
                ( 5, true ) if begun => break,
                _ => {}
            }

        }

        if mood == Mood::Vanish { return; }

        served += 1;

        let seen = pairs(&params);
        let wanted = |key: &str| seen.get(key).cloned().unwrap_or_default();
        let sum = body.iter().fold(0u64, |sum, byte| sum + u64::from(*byte));
        let text = format!("Content-Type: text/plain\r\n\r\n{}|{sum}|{}|{}|{serial}|{served}", body.len(), wanted("CONTENT_LENGTH"), wanted("REQUEST_METHOD"));
        let mut out = Vec::new();

        if mood == Mood::Noisy { record(&mut out, 7, b"PHP Warning: noise\n"); }

        for chunk in text.as_bytes().chunks(7) { record(&mut out, 6, chunk); }

        record(&mut out, 6, &[]);
        record(&mut out, 3, &[0; 8]);

        if stream.write_all(&out).is_err() { return; }

        if mood == Mood::Close {

            let _ = stream.shutdown(std::net::Shutdown::Write);

            while stream.read(&mut [0u8; 64]).is_ok_and(|count| count > 0) {}

            return;

        }

    }

}

fn php ( mood: Mood ) -> Php {

    let listener = TcpListener::bind("127.0.0.1:0").expect("fastcgi listener");
    let addr = listener.local_addr().expect("fastcgi addr");
    let accepted = Arc::new(AtomicUsize::new(0));
    let count = accepted.clone();

    std::thread::spawn(move || {

        for stream in listener.incoming() {

            let Ok(stream) = stream else { break; };
            let serial = count.fetch_add(1, Ordering::SeqCst) + 1;

            std::thread::spawn(move || serve(stream, mood, serial));

        }

    });

    Php { addr, accepted }

}

fn fronted ( php: SocketAddr, keepalive: usize, buffer: Option<bool>, check: impl FnOnce(&mut Http1) ) {

    let origin = Origin::start();
    let dir = std::env::temp_dir().join(format!("aegisx-fastcgi-{}-{}", std::process::id(), php.port()));

    std::fs::create_dir_all(&dir).expect("dir");

    let running = proxy(origin.addr, |config| {

        let mut pool = PoolConfig { backends: vec![BackendConfig { address: php.into(), protocol: Protocol::Fastcgi, ..BackendConfig::default() }], ..PoolConfig::default() };

        pool.options.keepalive = keepalive;

        config.pools.insert("php".to_string(), pool);
        config.routes.push(Route { name: "app".to_string(), path: "/".to_string(), upstream: "php".to_string(), root: Some(dir.clone()), buffer_request: buffer, ..Route::default() });

    });

    let mut client = Http1::connect(running.addr());

    check(&mut client);

    running.stop().expect("stop");

    let _ = std::fs::remove_dir_all(&dir);

}

#[test]
fn keepalive_serves_many_requests_over_one_connection () {

    let php = php(Mood::Keep);

    fronted(php.addr, 4, None, |client| {

        for round in 1..=5 {

            let reply = client.get("/index.php");

            assert_eq!(reply.status, 200);
            assert_eq!(reply.text(), format!("0|0|0|GET|1|{round}"));

        }

        let posted = client.request("POST", "/index.php", &[], b"name=core");

        assert_eq!(posted.text(), "9|903|9|POST|1|6");

    });

    assert_eq!(php.accepted.load(Ordering::SeqCst), 1);

}

#[test]
fn without_keepalive_every_request_dials () {

    let php = php(Mood::Keep);

    fronted(php.addr, 0, None, |client| {

        for round in 1..=3 { assert_eq!(client.get("/index.php").text(), format!("0|0|0|GET|{round}|1")); }

    });

    assert_eq!(php.accepted.load(Ordering::SeqCst), 3);

}

#[test]
fn a_parked_connection_the_backend_closed_is_replaced () {

    let php = php(Mood::Close);

    fronted(php.addr, 4, None, |client| {

        for round in 1..=3 {

            let reply = client.get("/index.php");

            assert_eq!(reply.status, 200);
            assert_eq!(reply.text(), format!("0|0|0|GET|{round}|1"));

        }

    });

    assert_eq!(php.accepted.load(Ordering::SeqCst), 3);

}

#[test]
fn uploads_cross_record_boundaries_streamed_or_buffered () {

    let body: Vec<u8> = (0..200_000u32).map(|index| (index % 251) as u8).collect();
    let sum = body.iter().fold(0u64, |sum, byte| sum + u64::from(*byte));

    for buffer in [Some(false), Some(true)] {

        let php = php(Mood::Keep);

        fronted(php.addr, 2, buffer, |client| {

            for round in 1..=2 {

                let reply = client.request("POST", "/upload.php", &[( "Content-Type", "application/octet-stream" )], &body);

                assert_eq!(reply.status, 200);
                assert_eq!(reply.text(), format!("200000|{sum}|200000|POST|1|{round}"));

            }

        });

    }

}

#[test]
fn chunked_uploads_are_measured_before_the_script_runs () {

    let php = php(Mood::Keep);

    fronted(php.addr, 0, Some(false), |client| {

        client.send(b"POST /index.php HTTP/1.1\r\nHost: test.local\r\nTransfer-Encoding: chunked\r\n\r\n4\r\ncore\r\n5\r\nproxy\r\n0\r\n\r\n");

        let reply = client.reply();

        assert_eq!(reply.status, 200);
        assert_eq!(reply.text(), "9|1003|9|POST|1|1");

    });

}

#[test]
fn stderr_is_kept_out_of_the_body () {

    let php = php(Mood::Noisy);

    fronted(php.addr, 0, None, |client| {

        let reply = client.get("/index.php");

        assert_eq!(reply.status, 200);
        assert_eq!(reply.text(), "0|0|0|GET|1|1");

    });

}

#[test]
fn a_backend_that_vanishes_or_refuses_answers_502 () {

    let php = php(Mood::Vanish);

    fronted(php.addr, 4, None, |client| {

        assert_eq!(client.get("/index.php").status, 502);
        assert_eq!(client.request("POST", "/index.php", &[], b"name=core").status, 502);

    });

    let gone = TcpListener::bind("127.0.0.1:0").expect("listener").local_addr().expect("addr");

    fronted(gone, 4, None, |client| assert_eq!(client.get("/index.php").status, 502));

}
