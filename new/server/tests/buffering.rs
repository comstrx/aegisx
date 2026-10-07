mod support;

use aegisx::config::Route;
use support::{Http1, Origin, proxy};

#[test]
fn buffered_responses_arrive_with_a_content_length () {

    let origin = Origin::start();
    let running = proxy(origin.addr, |config| { config.limits.buffer_responses = true; config.limits.response_buffer_bytes = 1_048_576; });
    let mut client = Http1::connect(running.addr());

    let reply = client.get("/chunked/4/4096/10");

    assert_eq!(reply.status, 200);
    assert_eq!(reply.header("content-length"), Some("16384"));
    assert_eq!(reply.header("transfer-encoding"), None);
    assert_eq!(reply.body, vec![b'c'; 16384]);

    running.stop().expect("stop");

}

#[test]
fn buffering_falls_back_to_streaming_past_the_cap () {

    let origin = Origin::start();
    let running = proxy(origin.addr, |config| {
        config.limits.response_buffer_bytes = 4096;
        config.routes.push(Route { name: "buffered".to_string(), path: "/".to_string(), upstream: "default".to_string(), buffer_response: Some(true), ..Route::default() });
    });
    let mut client = Http1::connect(running.addr());

    let reply = client.get("/chunked/8/4096/5");

    assert_eq!(reply.status, 200);
    assert_eq!(reply.header("content-length"), None);
    assert_eq!(reply.body, vec![b'c'; 32768]);

    let small = client.request("POST", "/echo", &[], b"tiny");

    assert_eq!(small.header("content-length"), Some("4"));
    assert_eq!(small.text(), "tiny");

    running.stop().expect("stop");

}

#[test]
fn large_buffered_uploads_spill_to_disk_and_arrive_whole () {

    let origin = Origin::start();
    let running = proxy(origin.addr, |config| {

        config.limits.max_body_bytes = 4 * 1_048_576;
        config.limits.spool_bytes = 4_096;
        config.routes.push(Route { name: "upload".to_string(), path: "/".to_string(), upstream: "default".to_string(), buffer_request: Some(true), ..Route::default() });

    });
    let mut client = Http1::connect(running.addr());
    let body: Vec<u8> = (0..1_500_000u32).map(|index| (index % 253) as u8).collect();

    for payload in [&body[..], &body[..4_096], &body[..4_097], &body[..0]] {

        let reply = client.request("POST", "/echo", &[], payload);

        assert_eq!(reply.status, 200);
        assert_eq!(reply.body.len(), payload.len());
        assert!(reply.body == payload, "the echoed upload differs");

    }

    let seen = origin.seen();

    assert_eq!(seen[0].header("content-length"), Some("1500000"));
    assert_eq!(seen[0].header("transfer-encoding"), None);

    running.stop().expect("stop");

}

#[test]
fn a_missing_spool_directory_fails_only_the_uploads_that_need_it () {

    let origin = Origin::start();
    let running = proxy(origin.addr, |config| {

        config.limits.spool_bytes = 1_024;
        config.limits.spool_dir = std::path::PathBuf::from("/nonexistent/aegisx-spool");
        config.routes.push(Route { name: "upload".to_string(), path: "/".to_string(), upstream: "default".to_string(), buffer_request: Some(true), ..Route::default() });

    });
    let mut client = Http1::connect(running.addr());

    assert_eq!(client.request("POST", "/echo", &[], &vec![b'm'; 1_024]).status, 200);
    assert_eq!(Http1::connect(running.addr()).request("POST", "/echo", &[], &vec![b'd'; 3_000]).status, 502);
    assert_eq!(client.request("POST", "/echo", &[], b"after").status, 200);

    running.stop().expect("stop");

}

