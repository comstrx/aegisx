mod support;

use std::net::IpAddr;
use std::path::PathBuf;

use aegisx::config::{Config, Route};
use aegisx::http::key::Hint;
use aegisx::http::variable::{Catalog, Kind, Recipe};
use support::{Http1, Origin, proxy};

#[derive(Clone, Copy)]
enum Slot {
    Empty,
    Node(usize),
    Data(usize),
}

fn text ( out: &mut Vec<u8>, value: &str ) {

    out.push((2 << 5) | value.len() as u8);
    out.extend_from_slice(value.as_bytes());

}

fn short ( out: &mut Vec<u8>, value: u16 ) {

    out.push((5 << 5) | 2);
    out.extend_from_slice(&value.to_be_bytes());

}

fn word ( out: &mut Vec<u8>, value: u32 ) {

    out.push((6 << 5) | 4);
    out.extend_from_slice(&value.to_be_bytes());

}

fn record ( country: &str, asn: u32 ) -> Vec<u8> {

    let mut out = vec![(7 << 5) | 2];

    text(&mut out, "country");
    out.push((7 << 5) | 1);
    text(&mut out, "iso_code");
    text(&mut out, country);
    text(&mut out, "asn");
    word(&mut out, asn);

    out

}

fn database ( networks: &[( [u8; 4], u8, Vec<u8> )] ) -> Vec<u8> {

    let mut nodes = vec![[Slot::Empty; 2]];
    let mut data = Vec::new();

    for ( address, prefix, record ) in networks {

        let ( bits, offset ) = ( u32::from_be_bytes(*address), data.len() );
        let mut at = 0;

        data.extend_from_slice(record);

        for depth in 0..u32::from(*prefix) {

            let side = ((bits >> (31 - depth)) & 1) as usize;

            if depth + 1 == u32::from(*prefix) { nodes[at][side] = Slot::Data(offset); break; }

            at = match nodes[at][side] {
                Slot::Node(next) => next,
                _ => { nodes.push([Slot::Empty; 2]); nodes[at][side] = Slot::Node(nodes.len() - 1); nodes.len() - 1 }
            };

        }

    }

    let count = nodes.len();
    let mut out = Vec::new();

    for slot in nodes.iter().flatten() {

        let value = match slot { Slot::Empty => count, Slot::Node(next) => *next, Slot::Data(offset) => count + 16 + offset };

        out.extend_from_slice(&(value as u32).to_be_bytes()[1..]);

    }

    out.extend_from_slice(&[0u8; 16]);
    out.extend_from_slice(&data);
    out.extend_from_slice(b"\xab\xcd\xefMaxMind.com");
    out.push((7 << 5) | 9);

    text(&mut out, "binary_format_major_version");
    short(&mut out, 2);
    text(&mut out, "binary_format_minor_version");
    short(&mut out, 0);
    text(&mut out, "build_epoch");
    out.extend_from_slice(&[8, 2]);
    out.extend_from_slice(&1_700_000_000u64.to_be_bytes());
    text(&mut out, "database_type");
    text(&mut out, "Test");
    text(&mut out, "description");
    out.push(7 << 5);
    text(&mut out, "ip_version");
    short(&mut out, 4);
    text(&mut out, "languages");
    out.extend_from_slice(&[0, 4]);
    text(&mut out, "node_count");
    word(&mut out, count as u32);
    text(&mut out, "record_size");
    short(&mut out, 24);

    out

}

fn fixture ( label: &str ) -> PathBuf {

    let path = std::env::temp_dir().join(format!("aegisx-geoip-{label}-{}.mmdb", std::process::id()));

    std::fs::write(&path, database(&[( [127, 0, 0, 0], 8, record("EG", 64_512) ), ( [10, 0, 0, 0], 8, record("SA", 64_513) )])).expect("mmdb");

    path

}

fn recipe ( path: &std::path::Path, field: &str ) -> Recipe {

    Recipe { kind: Kind::Mmdb, path: Some(path.to_path_buf()), field: field.to_string(), default: "ZZ".to_string(), ..Recipe::default() }

}

#[test]
fn databases_answer_text_and_number_fields_by_address () {

    let path = fixture("unit");
    let catalog = Catalog::compile(&[( "country".to_string(), recipe(&path, "country.iso_code") ), ( "asn".to_string(), recipe(&path, "asn") ), ( "city".to_string(), recipe(&path, "city.names.en") )].into_iter().collect()).expect("catalog");
    let ( headers, uri ) = ( http::HeaderMap::new(), http::Uri::from_static("/") );
    let seen = |name: &str, ip: &str| String::from_utf8_lossy(&catalog.value(catalog.index(name).expect("index"), Hint { ip: ip.parse::<IpAddr>().expect("ip"), secure: false, headers: &headers, uri: &uri })).into_owned();

    assert!(catalog.located);
    assert_eq!(seen("country", "127.0.0.1"), "EG");
    assert_eq!(seen("country", "10.20.30.40"), "SA");
    assert_eq!(seen("asn", "10.20.30.40"), "64513");
    assert_eq!(seen("country", "8.8.8.8"), "ZZ");
    assert_eq!(seen("country", "::1"), "ZZ");
    assert_eq!(seen("city", "127.0.0.1"), "ZZ");

    let _ = std::fs::remove_file(&path);

}

#[test]
fn bad_databases_and_fields_are_rejected () {

    let missing = recipe(std::path::Path::new("/nonexistent/aegisx.mmdb"), "country.iso_code");
    let broken = std::env::temp_dir().join(format!("aegisx-geoip-broken-{}.mmdb", std::process::id()));

    std::fs::write(&broken, b"not a database").expect("broken");

    assert!(Catalog::compile(&[( "country".to_string(), missing )].into_iter().collect()).is_err());
    assert!(Catalog::compile(&[( "country".to_string(), recipe(&broken, "country.iso_code") )].into_iter().collect()).is_err());

    let path = fixture("fields");

    assert!(Catalog::compile(&[( "country".to_string(), recipe(&path, "") )].into_iter().collect()).is_err());
    assert!(Catalog::compile(&[( "country".to_string(), recipe(&path, "a.b.c.d.e.f.g") )].into_iter().collect()).is_err());
    assert!(Catalog::compile(&[( "country".to_string(), Recipe { kind: Kind::Mmdb, field: "country.iso_code".to_string(), ..Recipe::default() } )].into_iter().collect()).is_err());

    let _ = std::fs::remove_file(&path);
    let _ = std::fs::remove_file(&broken);

}

#[test]
fn countries_route_requests_and_reach_the_upstream_as_headers () {

    let path = fixture("route");
    let origin = Origin::start();

    let running = proxy(origin.addr, |config| {

        config.variables.insert("country".to_string(), recipe(&path, "country.iso_code"));
        config.request_headers.insert("x-country".to_string(), "$country".to_string());
        config.routes.push(Route { name: "blocked".to_string(), path: "/".to_string(), upstream: "default".to_string(), deny: true, match_vars: [( "country".to_string(), "~^(SA|RU)$".to_string() )].into_iter().collect(), ..Route::default() });
        config.routes.push(Route { name: "home".to_string(), path: "/".to_string(), upstream: "default".to_string(), match_vars: [( "country".to_string(), "EG".to_string() )].into_iter().collect(), ..Route::default() });

    });

    let mut client = Http1::connect(running.addr());

    assert_eq!(client.get("/").status, 200);
    assert_eq!(origin.seen()[0].header("x-country"), Some("EG"));

    running.stop().expect("stop");

    let _ = std::fs::remove_file(&path);

}

#[test]
fn the_dsl_declares_databases_relative_to_the_config () {

    let path = fixture("dsl");
    let dir = path.parent().expect("dir");
    let name = path.file_name().expect("name").to_string_lossy();

    let config = Config::parse(&format!(r#"
        set_upstream("127.0.0.1:3000")
        add_geoip("country", {{ path = "{name}", field = "country.iso_code", default = "ZZ" }})
        add_route {{ name = "home", path = "/", match_vars = {{ country = "EG" }} }}
    "#), "geoip.lua", dir).expect("config");

    assert_eq!(config.variables["country"].path.as_deref(), Some(path.as_path()));
    assert!(Config::parse(r#"set_upstream("127.0.0.1:3000") add_geoip("country", { path = "missing.mmdb", field = "country.iso_code" })"#, "geoip.lua", dir).is_err());

    let _ = std::fs::remove_file(&path);

}
