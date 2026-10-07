mod support;

use std::collections::BTreeMap;
use std::path::Path;

use aegisx::config::{Config, Respond, Route};
use support::{Http1, Origin, proxy};

#[test]
fn routes_answer_with_a_fixed_response_without_touching_the_upstream () {

    let origin = Origin::start();
    let running = proxy(origin.addr, |config| {

        config.routes.push(Route { name: "health".to_string(), path: "/health".to_string(), exact: true, respond: Some(Respond { body: "ok".to_string(), ..Respond::default() }), ..Route::default() });
        config.routes.push(Route { name: "status".to_string(), path: "/status.json".to_string(), exact: true, respond: Some(Respond { status: 503, body: r#"{"up":false}"#.to_string(), content_type: "application/json".to_string() }), response_headers: BTreeMap::from([( "cache-control".to_string(), "no-store".to_string() )]), ..Route::default() });
        config.routes.push(Route { name: "all".to_string(), path: "/".to_string(), upstream: "default".to_string(), ..Route::default() });

    });
    let mut client = Http1::connect(running.addr());

    let health = client.get("/health");

    assert_eq!(health.status, 200);
    assert_eq!(health.text(), "ok");
    assert_eq!(health.header("content-type"), Some("text/plain; charset=utf-8"));
    assert_eq!(health.header("content-length"), Some("2"));
    assert!(health.header("x-request-id").is_some());

    let status = client.get("/status.json");

    assert_eq!(status.status, 503);
    assert_eq!(status.text(), r#"{"up":false}"#);
    assert_eq!(status.header("content-type"), Some("application/json"));
    assert_eq!(status.header("cache-control"), Some("no-store"));

    let head = client.request("HEAD", "/health", &[], b"");

    assert_eq!(head.status, 200);
    assert_eq!(head.header("content-length"), Some("2"));
    assert!(head.body.is_empty());

    assert_eq!(origin.accepted(), 0);
    assert_eq!(client.get("/other").status, 200);
    assert_eq!(origin.seen().len(), 1);

    running.stop().expect("stop");

}

#[test]
fn fixed_responses_need_no_upstream_and_are_validated () {

    let config = Config::parse(r#"
        add_route { name = "health", path = "/health", exact = true, respond = { body = "ok" } }
        add_route { name = "gone", path = "/old", respond = { status = 410, body = "<h1>gone</h1>", content_type = "text/html" } }
    "#, "respond.lua", Path::new("/tmp")).expect("parse");

    assert_eq!(config.routes[0].respond.as_ref().map(|respond| respond.status), Some(200));
    assert_eq!(config.routes[1].respond.as_ref().map(|respond| respond.content_type.as_str()), Some("text/html"));

    let error = Config::parse(r#"add_route { name = "bad", path = "/", respond = { status = 99 } }"#, "respond.lua", Path::new("/tmp")).expect_err("status out of range").to_string();

    assert!(error.contains("respond.status"), "{error}");

}

fn fastcgi_responder () -> std::net::SocketAddr {

    use std::io::{Read, Write};

    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("fastcgi listener");
    let addr = listener.local_addr().expect("fastcgi addr");

    std::thread::spawn(move || {

        for stream in listener.incoming() {

            let Ok(mut stream) = stream else { break; };
            let mut params: Vec<u8> = Vec::new();
            let mut body: Vec<u8> = Vec::new();

            loop {

                let mut header = [0u8; 8];

                if stream.read_exact(&mut header).is_err() { break; }

                let mut content = vec![0u8; usize::from(u16::from_be_bytes([header[4], header[5]])) + usize::from(header[6])];

                if stream.read_exact(&mut content).is_err() { break; }

                content.truncate(content.len() - usize::from(header[6]));

                match ( header[1], content.is_empty() ) {
                    ( 4, false ) => params.extend_from_slice(&content),
                    ( 5, false ) => body.extend_from_slice(&content),
                    ( 5, true ) => break,
                    _ => {}
                }

            }

            let mut pairs = std::collections::BTreeMap::new();
            let mut at = 0;

            while at < params.len() {

                let size = |at: &mut usize| { let first = params[*at]; if first < 128 { *at += 1; usize::from(first) } else { let wide = u32::from_be_bytes([params[*at] & 0x7f, params[*at + 1], params[*at + 2], params[*at + 3]]); *at += 4; wide as usize } };
                let ( name, value ) = ( size(&mut at), size(&mut at) );

                pairs.insert(String::from_utf8_lossy(&params[at..at + name]).into_owned(), String::from_utf8_lossy(&params[at + name..at + name + value]).into_owned());
                at += name + value;

            }

            let wanted = |key: &str| pairs.get(key).cloned().unwrap_or_default();
            let status = if wanted("SCRIPT_NAME") == "/missing.php" { "Status: 404 Not Found\r\n" } else { "" };
            let text = format!("{status}Content-Type: text/plain\r\nX-Powered-By: test\r\n\r\n{}|{}|{}|{}|{}|{}", wanted("SCRIPT_FILENAME"), wanted("PATH_INFO"), wanted("REQUEST_URI"), wanted("HTTP_X_TOKEN"), wanted("REQUEST_METHOD"), String::from_utf8_lossy(&body));
            let mut out = Vec::new();

            for chunk in text.as_bytes().chunks(11) {

                out.extend_from_slice(&[1, 6, 0, 1]);
                out.extend_from_slice(&(chunk.len() as u16).to_be_bytes());
                out.extend_from_slice(&[0, 0]);
                out.extend_from_slice(chunk);

            }

            out.extend_from_slice(&[1, 6, 0, 1, 0, 0, 0, 0, 1, 3, 0, 1, 0, 8, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]);

            let _ = stream.write_all(&out);
            let _ = stream.shutdown(std::net::Shutdown::Write);

            while stream.read(&mut [0u8; 64]).is_ok_and(|count| count > 0) {}

        }

    });

    addr

}

#[test]
fn fastcgi_pools_run_scripts_and_leave_static_files_to_the_proxy () {

    use aegisx::config::{BackendConfig, PoolConfig};
    use aegisx::http::upstream::Protocol;

    let php = fastcgi_responder();
    let origin = Origin::start();
    let dir = std::env::temp_dir().join(format!("aegisx-fcgi-{}", std::process::id()));

    std::fs::create_dir_all(&dir).expect("dir");
    std::fs::write(dir.join("app.css"), b"body{}").expect("css");
    std::fs::write(dir.join("index.php"), b"<?php secret").expect("php");

    let running = proxy(origin.addr, |config| {

        config.pools.insert("php".to_string(), PoolConfig { backends: vec![BackendConfig { address: php.into(), protocol: Protocol::Fastcgi, ..BackendConfig::default() }], ..PoolConfig::default() });
        config.routes.push(Route { name: "app".to_string(), path: "/".to_string(), upstream: "php".to_string(), root: Some(dir.clone()), ..Route::default() });

    });
    let mut client = Http1::connect(running.addr());
    let root = dir.to_string_lossy().into_owned();

    let front = client.request("POST", "/users/7?tab=posts", &[( "X-Token", "abc" )], b"name=core");

    assert_eq!(front.status, 200);
    assert_eq!(front.text(), format!("{root}/index.php||/users/7?tab=posts|abc|POST|name=core"));
    assert_eq!(front.header("x-powered-by"), Some("test"));

    let direct = client.get("/tools/run.php/extra/path");

    assert_eq!(direct.text(), format!("{root}/tools/run.php|/extra/path|/tools/run.php/extra/path||GET|"));
    assert_eq!(client.get("/missing.php").status, 404);
    assert_eq!(client.get("/app.css").text(), "body{}");
    assert!(client.get("/index.php").text().starts_with(&format!("{root}/index.php|")));

    running.stop().expect("stop");

    let _ = std::fs::remove_dir_all(&dir);

    assert!(aegisx::config::Config::parse(r#"add_upstream("php", { address = "127.0.0.1:9000", protocol = "fastcgi" }) add_route { name = "app", path = "/", upstream = "php" }"#, "fcgi.lua", std::path::Path::new("/tmp")).is_err());
    assert!(aegisx::config::Config::parse(r#"add_upstream("php", { address = "unix:/run/php/php-fpm.sock", protocol = "fastcgi" }) add_route { name = "app", path = "/", upstream = "php", root = "/srv/app/public" }"#, "fcgi.lua", std::path::Path::new("/tmp")).is_ok());

}
