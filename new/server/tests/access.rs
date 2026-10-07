mod support;

use std::fs;
use std::path::PathBuf;
use std::thread;
use std::time::Duration;

use aegisx::config::AccessFormat;
use aegisx::core::time::Clock;
use support::{Http1, Origin, proxy};

fn scratch ( name: &str ) -> PathBuf {

    let dir = std::env::temp_dir().join(format!("aegisx-access-{}", std::process::id()));

    fs::create_dir_all(&dir).expect("scratch dir");

    dir.join(format!("{name}.log"))

}

fn lines ( path: &PathBuf ) -> Vec<String> {

    fs::read_to_string(path).expect("access log").lines().map(str::to_owned).collect()

}

#[test]
fn combined_lines_cover_served_and_rejected_requests () {

    let origin = Origin::start();
    let path = scratch("combined");
    let running = proxy(origin.addr, |config| { config.access.path = path.clone(); config.limits.max_body_bytes = 1024; });
    let mut client = Http1::connect(running.addr());

    let served = client.request("POST", "/echo", &[( "User-Agent", "bench/1.0 \"quoted\"" ), ( "Referer", "https://example.test/x" )], b"hello world");

    assert_eq!(served.status, 200);

    let rejected = client.request("POST", "/echo", &[], &vec![b'x'; 4096]);

    assert_eq!(rejected.status, 413);

    running.stop().expect("stop");

    let lines = lines(&path);

    assert_eq!(lines.len(), 2, "{lines:?}");
    assert!(lines[0].starts_with("127.0.0.1 - - ["), "{}", lines[0]);
    assert!(lines[0].contains("] \"POST /echo HTTP/1.1\" 200 11 \"https://example.test/x\" \"bench/1.0 \\\"quoted\\\"\" "), "{}", lines[0]);

    let tail: Vec<&str> = lines[0].rsplitn(4, ' ').collect();

    assert_ne!(tail[0], "-", "request id missing: {}", lines[0]);
    assert_eq!(tail[1], "default");
    assert_eq!(tail[2], origin.addr.to_string());
    assert!(tail[3].ends_with(|byte: char| byte.is_ascii_digit()), "request time missing: {}", lines[0]);

    assert!(lines[1].contains("\"POST /echo HTTP/1.1\" 413 0 \"-\" \"-\" "), "{}", lines[1]);
    assert!(lines[1].contains(" - default "), "{}", lines[1]);

}

#[test]
fn json_lines_carry_every_field () {

    let origin = Origin::start();
    let path = scratch("json");
    let running = proxy(origin.addr, |config| { config.access.path = path.clone(); config.access.format = AccessFormat::Json; });
    let mut client = Http1::connect(running.addr());

    let reply = client.request("POST", "/echo?x=1&y=two", &[( "User-Agent", "tab\there" )], b"payload");

    assert_eq!(reply.status, 200);

    let missing = client.get("/%2e%2e/etc/passwd");

    assert_eq!(missing.status, 400);

    running.stop().expect("stop");

    let lines = lines(&path);

    assert_eq!(lines.len(), 2, "{lines:?}");

    let served: serde_json::Value = serde_json::from_str(&lines[0]).expect("json line");

    assert_eq!(served["peer"], "127.0.0.1");
    assert_eq!(served["method"], "POST");
    assert_eq!(served["target"], "/echo?x=1&y=two");
    assert_eq!(served["version"], "HTTP/1.1");
    assert_eq!(served["status"], 200);
    assert_eq!(served["bytes_sent"], 7);
    assert_eq!(served["bytes_received"], 7);
    assert_eq!(served["user_agent"], "tab\there");
    assert_eq!(served["referer"], serde_json::Value::Null);
    assert_eq!(served["backend"], origin.addr.to_string());
    assert_eq!(served["route"], "default");
    assert!(served["request_id"].as_str().is_some_and(|id| !id.is_empty()));
    assert!(served["request_time_us"].as_u64().is_some_and(|us| us > 0));
    assert!(served["upstream_time_us"].as_u64().is_some_and(|us| us > 0));
    assert_eq!(served["attempts"], 1);

    let time = served["time"].as_str().expect("time");
    let bytes = time.as_bytes();

    assert_eq!(time.len(), 26, "{time}");
    assert_eq!(( bytes[2], bytes[6], bytes[11], bytes[14], bytes[17] ), ( b'/', b'/', b':', b':', b':' ), "{time}");
    assert!(time.ends_with(" +0000"), "{time}");

    let rejected: serde_json::Value = serde_json::from_str(&lines[1]).expect("json line");

    assert_eq!(rejected["status"], 400);
    assert_eq!(rejected["route"], serde_json::Value::Null);
    assert_eq!(rejected["backend"], serde_json::Value::Null);
    assert_eq!(rejected["target"], "/%2e%2e/etc/passwd");

}

#[test]
fn rotated_files_are_reopened_on_the_next_flush () {

    let origin = Origin::start();
    let path = scratch("rotate");
    let rotated = scratch("rotate.1");
    let running = proxy(origin.addr, |config| { config.access.path = path.clone(); config.access.flush_ms = 20; });
    let mut client = Http1::connect(running.addr());

    assert_eq!(client.get("/first").status, 200);

    thread::sleep(Duration::from_millis(200));
    fs::rename(&path, &rotated).expect("rotate");

    assert_eq!(client.get("/second").status, 200);

    running.stop().expect("stop");

    let old = lines(&rotated);
    let new = lines(&path);

    assert_eq!(old.len(), 1, "{old:?}");
    assert!(old[0].contains("\"GET /first HTTP/1.1\" 200"), "{}", old[0]);
    assert_eq!(new.len(), 1, "{new:?}");
    assert!(new[0].contains("\"GET /second HTTP/1.1\" 200"), "{}", new[0]);

}

#[test]
fn streamed_bodies_are_counted_after_they_finish () {

    let origin = Origin::start();
    let path = scratch("streamed");
    let running = proxy(origin.addr, |config| config.access.path = path.clone());
    let mut client = Http1::connect(running.addr());

    let reply = client.get("/chunked/4/8192/5");

    assert_eq!(reply.status, 200);
    assert_eq!(reply.body.len(), 4 * 8192);

    running.stop().expect("stop");

    let lines = lines(&path);

    assert_eq!(lines.len(), 1, "{lines:?}");
    assert!(lines[0].contains("\"GET /chunked/4/8192/5 HTTP/1.1\" 200 32768 "), "{}", lines[0]);

}

#[test]
fn unreachable_log_paths_fail_at_boot () {

    let origin = Origin::start();
    let mut config = aegisx::config::Config { listen: support::free_port(), ..aegisx::config::Config::default() };

    config.set_upstream(origin.addr);
    config.runtime.workers = 1;
    config.runtime.pin = false;
    config.access.path = PathBuf::from("/nonexistent-dir-for-aegisx/access.log");

    let error = aegisx::app::Boot::start(config).err().expect("boot must fail");

    assert!(error.to_string().contains("set_access_log"), "{error}");

}

#[test]
fn civil_dates_match_the_proleptic_gregorian_calendar () {

    assert_eq!(Clock::civil(0), ( 1970, 1, 1 ));
    assert_eq!(Clock::civil(-1), ( 1969, 12, 31 ));
    assert_eq!(Clock::civil(11_016), ( 2000, 2, 29 ));
    assert_eq!(Clock::civil(20_729), ( 2026, 10, 3 ));
    assert_eq!(Clock::civil(47_541), ( 2100, 3, 1 ));

}

#[test]
fn custom_patterns_render_nginx_style_variables_and_captured_headers () {

    let origin = Origin::start();
    let path = scratch("pattern");
    let running = proxy(origin.addr, |config| {
        config.access.path = path.clone();
        config.access.pattern = "$remote_addr $remote_user [$time_local] \"$request\" $status $body_bytes_sent $request_time $upstream_addr $route $scheme $http_x_tenant ${http_user_agent} $host $msec $time_iso8601 $request_method $uri $args $request_length $server_protocol $upstream_response_time $upstream_status $upstream_attempts".to_string();
    });
    let mut client = Http1::connect(running.addr());

    let auth = {

        use base64::Engine;

        format!("Basic {}", base64::engine::general_purpose::STANDARD.encode("alice:secret"))

    };

    assert_eq!(client.request("POST", "/echo?x=1&y=2", &[( "X-Tenant", "acme \"inc\"" ), ( "User-Agent", "bench/2" ), ( "Authorization", &auth )], b"hello").status, 200);
    assert_eq!(client.get("/plain").status, 200);

    running.stop().expect("stop");

    let lines = lines(&path);

    assert_eq!(lines.len(), 2, "{lines:?}");

    let first = &lines[0];

    assert!(first.starts_with("127.0.0.1 alice ["), "{first}");
    assert!(first.contains("] \"POST /echo?x=1&y=2 HTTP/1.1\" 200 5 0."), "{first}");
    assert!(first.contains(&format!(" {} default http acme \\\"inc\\\" bench/2 test.local ", origin.addr)), "{first}");
    assert!(first.contains(" POST /echo x=1&y=2 5 HTTP/1.1 0.") && first.ends_with(" 200 1"), "{first}");
    assert!(first.contains("T") && first.contains("+00:00 "), "{first}");

    let second = &lines[1];

    assert!(second.starts_with("127.0.0.1 - ["), "{second}");
    assert!(second.contains(" http - - "), "{second}");
    assert!(second.contains(" GET /plain  0 HTTP/1.1 0.") && second.ends_with(" 200 1"), "{second}");

    let bad = aegisx::config::Config::parse(r#"set_upstream("127.0.0.1:3000") set_access_log { path = "stdout", pattern = "$nope" }"#, "p.lua", std::path::Path::new("/tmp"));

    assert!(bad.is_ok(), "unknown variables are rejected when the log is opened, not at parse time");

}


#[test]
fn a_status_floor_keeps_only_the_failures () {

    let origin = Origin::start();
    let path = scratch("floor");
    let running = proxy(origin.addr, |config| { config.access.path = path.clone(); config.access.min_status = 400; config.limits.max_body_bytes = 1024; });
    let mut client = Http1::connect(running.addr());

    assert_eq!(client.get("/fine").status, 200);
    assert_eq!(client.request("POST", "/echo", &[], &vec![b'x'; 4096]).status, 413);
    assert_eq!(client.get("/fine-again").status, 200);

    running.stop().expect("stop");

    let lines = lines(&path);

    assert_eq!(lines.len(), 1, "{lines:?}");
    assert!(lines[0].contains("\" 413 "), "{}", lines[0]);

}

#[test]
fn syslog_sinks_receive_one_datagram_per_line () {

    let origin = Origin::start();
    let collector = std::net::UdpSocket::bind("127.0.0.1:0").expect("collector");

    collector.set_read_timeout(Some(std::time::Duration::from_secs(5))).expect("timeout");

    let target = collector.local_addr().expect("addr");
    let running = proxy(origin.addr, |config| config.access.path = std::path::PathBuf::from(format!("syslog://{target}")));

    assert_eq!(Http1::connect(running.addr()).get("/logged").status, 200);

    running.stop().expect("stop");

    let mut buffer = [0u8; 2048];
    let size = collector.recv(&mut buffer).expect("datagram");
    let line = String::from_utf8_lossy(&buffer[..size]);

    assert!(line.starts_with("<134>1 - - aegisx - - - "), "{line}");
    assert!(line.contains("GET /logged"), "{line}");

}

#[test]
fn size_rotation_keeps_whole_lines_and_a_bounded_history () {

    let origin = Origin::start();
    let path = scratch("rotating");
    let running = proxy(origin.addr, |config| {

        config.access.path = path.clone();
        config.access.rotate_bytes = 4_096;
        config.access.rotate_keep = 2;
        config.access.flush_ms = 10;

    });
    let mut client = Http1::connect(running.addr());

    for round in 0..400 {

        assert_eq!(client.get(&format!("/rotate/{round}")).status, 200);

        if round % 50 == 49 { thread::sleep(Duration::from_millis(40)); }

    }

    running.stop().expect("stop");

    let rotated = |index: usize| PathBuf::from(format!("{}.{index}", path.display()));

    assert!(rotated(1).exists() && rotated(2).exists(), "two rotated files are kept");
    assert!(!rotated(3).exists(), "history is capped at rotate_keep");

    let kept: Vec<String> = [rotated(2), rotated(1), path.clone()].iter().flat_map(lines).collect();

    assert!(!kept.is_empty() && kept.len() <= 400);
    assert!(kept.iter().all(|line| line.starts_with("127.0.0.1 - - [") && line.contains("\"GET /rotate/")), "every kept line is whole");
    assert!(kept.last().is_some_and(|line| line.contains("\"GET /rotate/399 ")), "the newest line is in the live file");

}

#[test]
fn rotation_settings_are_validated () {

    let parse = |body: &str| aegisx::config::Config::parse(&format!("set_upstream(\"127.0.0.1:3000\") set_access_log {{ {body} }}"), "access.lua", std::path::Path::new("/tmp"));

    assert!(parse("path = \"a.log\", rotate_bytes = 1048576, rotate_keep = 5, rotate_compress = true").is_ok());
    assert!(parse("path = \"a.log\", rotate_every = \"daily\"").is_ok());
    assert!(parse("path = \"a.log\", rotate_bytes = 1048576, rotate_every = \"daily\"").is_err());
    assert!(parse("path = \"stdout\", rotate_every = \"daily\"").is_err());
    assert!(parse("path = \"a.log\", rotate_bytes = 100").is_err());
    assert!(parse("path = \"a.log\", rotate_every = \"sometimes\"").is_err());

}

