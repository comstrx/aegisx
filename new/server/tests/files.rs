mod support;

use std::fs;
use std::path::{Path, PathBuf};

use aegisx::config::Route;
use support::{Http1, Origin, proxy};

fn site ( name: &str ) -> PathBuf {

    let dir = std::env::temp_dir().join(format!("aegisx-files-{}-{name}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);

    fs::create_dir_all(dir.join("assets")).expect("assets dir");
    fs::create_dir_all(dir.join("docs")).expect("docs dir");
    fs::create_dir_all(dir.join("bare")).expect("bare dir");
    fs::write(dir.join("index.html"), "<h1>home</h1>").expect("index");
    fs::write(dir.join("docs/index.html"), "<h1>docs</h1>").expect("docs index");
    fs::write(dir.join("bare/x.txt"), "x").expect("bare file");
    fs::write(dir.join("a b.txt"), "spaced").expect("spaced file");
    fs::write(dir.join("assets/app.js"), payload()).expect("app.js");
    fs::write(dir.join("empty.css"), "").expect("empty");

    dir

}

fn payload () -> Vec<u8> {

    (0..204_800u32).map(|index| (index % 251) as u8).collect()

}

fn files ( root: &Path, path: &str, strip: bool ) -> Route {

    Route { name: "files".to_string(), path: path.to_string(), root: Some(root.to_path_buf()), strip_prefix: strip, cache_control: Some("public, max-age=600".to_string()), ..Route::default() }

}

#[test]
fn serves_files_with_metadata_headers () {

    let origin = Origin::start();
    let root = site("meta");
    let running = proxy(origin.addr, |config| config.routes.push(files(&root, "/", false)));
    let mut client = Http1::connect(running.addr());

    let reply = client.get("/assets/app.js");

    assert_eq!(reply.status, 200);
    assert_eq!(reply.header("content-type"), Some("text/javascript"));
    assert_eq!(reply.header("content-length"), Some("204800"));
    assert_eq!(reply.header("accept-ranges"), Some("bytes"));
    assert_eq!(reply.header("cache-control"), Some("public, max-age=600"));
    assert!(reply.header("etag").is_some_and(|etag| etag.starts_with('"') && etag.ends_with("-32000\"")), "{:?}", reply.header("etag"));
    assert!(reply.header("last-modified").is_some_and(|date| date.ends_with(" GMT")), "{:?}", reply.header("last-modified"));
    assert!(reply.header("x-request-id").is_some());
    assert_eq!(reply.body, payload());

    let home = client.get("/");

    assert_eq!(home.status, 200);
    assert_eq!(home.header("content-type"), Some("text/html"));
    assert_eq!(home.text(), "<h1>home</h1>");

    let empty = client.get("/empty.css");

    assert_eq!(empty.status, 200);
    assert_eq!(empty.header("content-length"), Some("0"));

    let head = client.request("HEAD", "/assets/app.js", &[], b"");

    assert_eq!(head.status, 200);
    assert_eq!(head.header("content-length"), Some("204800"));
    assert!(head.body.is_empty());

    let again = client.get("/a%20b.txt");

    assert_eq!(again.status, 200);
    assert_eq!(again.text(), "spaced");
    assert_eq!(origin.seen().len(), 0);

    running.stop().expect("stop");

}

#[test]
fn conditional_requests_answer_304 () {

    let origin = Origin::start();
    let root = site("conditional");
    let running = proxy(origin.addr, |config| config.routes.push(files(&root, "/", false)));
    let mut client = Http1::connect(running.addr());

    let first = client.get("/index.html");
    let etag = first.header("etag").expect("etag").to_string();
    let modified = first.header("last-modified").expect("last-modified").to_string();

    let matched = client.request("GET", "/index.html", &[( "If-None-Match", &etag )], b"");

    assert_eq!(matched.status, 304);
    assert_eq!(matched.header("etag").map(str::to_string), Some(etag.clone()));
    assert_eq!(matched.header("content-length"), None);
    assert!(matched.body.is_empty());

    let weak = client.request("GET", "/index.html", &[( "If-None-Match", &format!("\"other\", W/{etag}") )], b"");

    assert_eq!(weak.status, 304);

    let dated = client.request("GET", "/index.html", &[( "If-Modified-Since", &modified )], b"");

    assert_eq!(dated.status, 304);

    let stale = client.request("GET", "/index.html", &[( "If-Modified-Since", "Sat, 01 Jan 2000 00:00:00 GMT" )], b"");

    assert_eq!(stale.status, 200);

    let mismatch = client.request("GET", "/index.html", &[( "If-None-Match", "\"nope\"" ), ( "If-Modified-Since", &modified )], b"");

    assert_eq!(mismatch.status, 200);

    running.stop().expect("stop");

}

#[test]
fn byte_ranges_return_partial_content () {

    let origin = Origin::start();
    let root = site("ranges");
    let running = proxy(origin.addr, |config| config.routes.push(files(&root, "/", false)));
    let mut client = Http1::connect(running.addr());
    let data = payload();

    let middle = client.request("GET", "/assets/app.js", &[( "Range", "bytes=100-199" )], b"");

    assert_eq!(middle.status, 206);
    assert_eq!(middle.header("content-range"), Some("bytes 100-199/204800"));
    assert_eq!(middle.header("content-length"), Some("100"));
    assert_eq!(middle.body, data[100..200].to_vec());

    let tail = client.request("GET", "/assets/app.js", &[( "Range", "bytes=-100" )], b"");

    assert_eq!(tail.status, 206);
    assert_eq!(tail.header("content-range"), Some("bytes 204700-204799/204800"));
    assert_eq!(tail.body, data[204_700..].to_vec());

    let open = client.request("GET", "/assets/app.js", &[( "Range", "bytes=204700-" )], b"");

    assert_eq!(open.status, 206);
    assert_eq!(open.body.len(), 100);

    let clamped = client.request("GET", "/assets/app.js", &[( "Range", "bytes=204790-999999" )], b"");

    assert_eq!(clamped.status, 206);
    assert_eq!(clamped.header("content-range"), Some("bytes 204790-204799/204800"));

    let beyond = client.request("GET", "/assets/app.js", &[( "Range", "bytes=999999-" )], b"");

    assert_eq!(beyond.status, 416);
    assert_eq!(beyond.header("content-range"), Some("bytes */204800"));

    let multi = client.request("GET", "/assets/app.js", &[( "Range", "bytes=0-1,5-6" )], b"");

    assert_eq!(multi.status, 206);
    assert!(multi.header("content-type").is_some_and(|kind| kind.starts_with("multipart/byteranges; boundary=")));

    let changed = client.request("GET", "/assets/app.js", &[( "Range", "bytes=0-9" ), ( "If-Range", "\"stale\"" )], b"");

    assert_eq!(changed.status, 200);

    let etag = changed.header("etag").expect("etag").to_string();
    let same = client.request("GET", "/assets/app.js", &[( "Range", "bytes=0-9" ), ( "If-Range", &etag )], b"");

    assert_eq!(same.status, 206);
    assert_eq!(same.body, data[..10].to_vec());

    running.stop().expect("stop");

}

#[test]
fn directories_use_the_index_or_redirect_to_a_slash () {

    let origin = Origin::start();
    let root = site("dirs");
    let running = proxy(origin.addr, |config| config.routes.push(files(&root, "/", false)));
    let mut client = Http1::connect(running.addr());

    let bare = client.get("/docs?tab=1");

    assert_eq!(bare.status, 301);
    assert_eq!(bare.header("location"), Some("/docs/?tab=1"));

    let indexed = client.get("/docs/");

    assert_eq!(indexed.status, 200);
    assert_eq!(indexed.text(), "<h1>docs</h1>");

    assert_eq!(client.get("/bare/").status, 404);

    running.stop().expect("stop");

    let running = proxy(origin.addr, |config| config.routes.push(Route { index: Some(String::new()), ..files(&root, "/", false) }));
    let mut client = Http1::connect(running.addr());

    assert_eq!(client.get("/docs/").status, 403);
    assert_eq!(client.get("/bare/x.txt").status, 200);

    running.stop().expect("stop");

}

#[test]
fn rejects_what_must_not_be_served () {

    let origin = Origin::start();
    let root = site("rejects");
    let running = proxy(origin.addr, |config| config.routes.push(files(&root, "/", false)));
    let mut client = Http1::connect(running.addr());

    assert_eq!(client.get("/missing.txt").status, 404);
    assert_eq!(client.get("/assets/app.js/extra").status, 404);
    assert_eq!(client.get("/%2e%2e/%2e%2e/etc/passwd").status, 400);
    assert_eq!(client.get("/../etc/passwd").status, 400);
    assert_eq!(client.get("/a%00b.txt").status, 400);

    let post = client.request("POST", "/index.html", &[], b"data");

    assert_eq!(post.status, 405);
    assert_eq!(post.header("allow"), Some("GET, HEAD"));
    assert_eq!(origin.seen().len(), 0);

    running.stop().expect("stop");

}

#[test]
fn strip_prefix_works_as_alias () {

    let origin = Origin::start();
    let root = site("alias");
    let running = proxy(origin.addr, |config| {
        config.routes.push(files(&root, "/static", true));
        config.routes.push(Route { name: "all".to_string(), path: "/".to_string(), upstream: "default".to_string(), ..Route::default() });
    });
    let mut client = Http1::connect(running.addr());

    let reply = client.get("/static/assets/app.js");

    assert_eq!(reply.status, 200);
    assert_eq!(reply.body.len(), 204_800);
    assert_eq!(client.get("/static/").status, 200);
    assert_eq!(client.get("/assets/app.js").status, 200);
    assert_eq!(origin.seen().len(), 1);

    running.stop().expect("stop");

}

#[test]
fn static_responses_are_access_logged_with_bytes_sent () {

    let origin = Origin::start();
    let root = site("logged");
    let log = root.join("access.log");
    let running = proxy(origin.addr, |config| { config.routes.push(files(&root, "/", false)); config.access.path = log.clone(); });
    let mut client = Http1::connect(running.addr());

    assert_eq!(client.get("/assets/app.js").status, 200);
    assert_eq!(client.request("HEAD", "/assets/app.js", &[], b"").status, 200);
    assert_eq!(client.get("/missing").status, 404);

    running.stop().expect("stop");

    let text = fs::read_to_string(&log).expect("access log");
    let lines: Vec<&str> = text.lines().collect();

    assert_eq!(lines.len(), 3, "{text}");
    assert!(lines[0].contains("\"GET /assets/app.js HTTP/1.1\" 200 204800 "), "{}", lines[0]);
    assert!(lines[1].contains("\"HEAD /assets/app.js HTTP/1.1\" 200 0 "), "{}", lines[1]);
    assert!(lines[2].contains("\"GET /missing HTTP/1.1\" 404 0 "), "{}", lines[2]);
    assert!(lines[0].contains(" - files "), "{}", lines[0]);

}

#[test]
fn missing_roots_fail_at_boot () {

    let origin = Origin::start();
    let mut config = aegisx::config::Config { listen: support::free_port(), ..aegisx::config::Config::default() };

    config.set_upstream(origin.addr);
    config.runtime.workers = 1;
    config.runtime.pin = false;
    config.routes.push(files(Path::new("/nonexistent-aegisx-root"), "/", false));

    let error = aegisx::app::Boot::start(config).err().expect("boot must fail").to_string();

    assert!(error.contains("root"), "{error}");

}

#[test]
fn multipart_ranges_come_from_the_memory_cache () {

    let origin = Origin::start();
    let root = site("multipart");
    let running = proxy(origin.addr, |config| config.routes.push(files(&root, "/", false)));
    let mut client = Http1::connect(running.addr());
    let data = payload();

    let reply = client.request("GET", "/assets/app.js", &[( "Range", "bytes=0-9,20-29,-5" )], b"");

    assert_eq!(reply.status, 206);

    let kind = reply.header("content-type").expect("content type").to_string();
    let boundary = kind.strip_prefix("multipart/byteranges; boundary=").expect("boundary").to_string();
    let text = String::from_utf8_lossy(&reply.body).into_owned();
    let parts: Vec<&str> = text.split(&format!("--{boundary}")).filter(|part| part.contains("Content-Range")).collect();

    assert_eq!(parts.len(), 3, "{text}");
    assert!(parts[0].contains("Content-Range: bytes 0-9/204800"), "{}", parts[0]);
    assert!(parts[1].contains("Content-Range: bytes 20-29/204800"), "{}", parts[1]);
    assert!(parts[2].contains("Content-Range: bytes 204795-204799/204800"), "{}", parts[2]);
    assert!(parts[0].contains("Content-Type: text/javascript"), "{}", parts[0]);
    assert!(text.ends_with(&format!("--{boundary}--\r\n")), "{text}");
    assert_eq!(reply.header("content-length").and_then(|value| value.parse::<usize>().ok()), Some(reply.body.len()));

    let body_bytes = &reply.body;
    let first = body_bytes.windows(4).position(|window| window == b"\r\n\r\n").map(|at| &body_bytes[at + 4..at + 14]).expect("first part");

    assert_eq!(first, &data[0..10]);

    running.stop().expect("stop");

}

#[test]
fn cached_files_revalidate_after_the_valid_window () {

    let origin = Origin::start();
    let root = site("revalidate");
    let running = proxy(origin.addr, |config| { config.files.valid_ms = 300; config.routes.push(files(&root, "/", false)); });
    let mut client = Http1::connect(running.addr());

    assert_eq!(client.get("/index.html").text(), "<h1>home</h1>");

    fs::write(root.join("index.html"), "<h1>changed home</h1>").expect("rewrite");

    assert_eq!(client.get("/index.html").text(), "<h1>home</h1>", "cache served fresh content inside the valid window");

    std::thread::sleep(std::time::Duration::from_millis(1_200));

    assert_eq!(client.get("/index.html").text(), "<h1>changed home</h1>");
    assert_eq!(client.get("/index.html").header("content-length"), Some("21"));

    running.stop().expect("stop");

}

#[test]
fn autoindex_lists_directories_without_an_index () {

    let origin = Origin::start();
    let root = site("autoindex");
    let running = proxy(origin.addr, |config| {
        config.routes.push(Route { name: "listed".to_string(), path: "/listed".to_string(), root: Some(root.clone()), strip_prefix: true, index: Some(String::new()), autoindex: true, ..Route::default() });
        config.routes.push(Route { name: "closed".to_string(), path: "/closed".to_string(), root: Some(root.clone()), strip_prefix: true, index: Some(String::new()), ..Route::default() });
        config.routes.push(Route { name: "mixed".to_string(), path: "/mixed".to_string(), root: Some(root.clone()), strip_prefix: true, autoindex: true, ..Route::default() });
    });
    let mut client = Http1::connect(running.addr());

    let listing = client.get("/listed/");

    assert_eq!(listing.status, 200);
    assert_eq!(listing.header("content-type"), Some("text/html; charset=utf-8"));

    let html = listing.text();

    assert!(html.contains("<title>Index of /listed/</title>"), "{html}");
    assert!(html.contains("<a href=\"assets/\">assets/</a>"), "{html}");
    assert!(html.contains("<a href=\"a%20b.txt\">a b.txt</a>"), "{html}");
    assert!(html.find("assets/").unwrap_or(0) < html.find("a b.txt").unwrap_or(0), "directories first: {html}");

    assert_eq!(client.get("/listed/bare").status, 301);
    assert_eq!(client.get("/listed/bare/").status, 200);
    assert_eq!(client.get("/closed/").status, 403);
    assert_eq!(client.get("/mixed/docs/").text(), "<h1>docs</h1>");
    assert!(client.get("/mixed/bare/").text().contains("x.txt"));

    let head = client.request("HEAD", "/listed/", &[], b"");

    assert_eq!(head.status, 200);
    assert!(head.body.is_empty());

    running.stop().expect("stop");

}


#[test]
fn precompressed_siblings_are_negotiated () {

    let origin = Origin::start();
    let root = site("packed");

    fs::write(root.join("assets/app.js.br"), b"brotli-bytes").expect("br");
    fs::write(root.join("assets/app.js.gz"), b"gzip-bytes").expect("gz");

    let running = proxy(origin.addr, |config| { config.files.precompressed = true; config.routes.push(files(&root, "/", false)); });
    let mut client = Http1::connect(running.addr());

    let brotli = client.request("GET", "/assets/app.js", &[( "accept-encoding", "gzip, br" )], b"");

    assert_eq!(brotli.status, 200);
    assert_eq!(brotli.header("content-encoding"), Some("br"));
    assert_eq!(brotli.header("vary"), Some("accept-encoding"));
    assert_eq!(brotli.header("accept-ranges"), None);
    assert!(brotli.header("content-type").is_some_and(|kind| kind.contains("javascript")));
    assert_eq!(brotli.body, b"brotli-bytes");

    let gzip = client.request("GET", "/assets/app.js", &[( "accept-encoding", "gzip" )], b"");

    assert_eq!(gzip.header("content-encoding"), Some("gzip"));
    assert_eq!(gzip.body, b"gzip-bytes");
    assert_ne!(gzip.header("etag"), brotli.header("etag"));

    let etag = brotli.header("etag").expect("etag").to_string();
    let cached = client.request("GET", "/assets/app.js", &[( "accept-encoding", "br" ), ( "if-none-match", etag.as_str() )], b"");

    assert_eq!(cached.status, 304);

    let plain = client.get("/assets/app.js");

    assert_eq!(plain.header("content-encoding"), None);
    assert_eq!(plain.header("vary"), Some("accept-encoding"));
    assert_eq!(plain.body, payload());

    let ranged = client.request("GET", "/assets/app.js", &[( "accept-encoding", "br" ), ( "range", "bytes=0-9" )], b"");

    assert_eq!(ranged.status, 206);
    assert_eq!(ranged.header("content-encoding"), None);
    assert_eq!(ranged.body.len(), 10);

    let untouched = client.request("GET", "/index.html", &[( "accept-encoding", "br" )], b"");

    assert_eq!(untouched.header("content-encoding"), None);
    assert_eq!(untouched.header("vary"), None);

    running.stop().expect("stop");

}
