use std::path::Path;

use aegisx::config::Config;
use aegisx::http::Congestion;

fn parse ( source: &str ) -> Result<Config, String> {

    Config::parse(source, "tuning.lua", Path::new("/tmp")).map_err(|error| error.to_string())

}

#[test]
fn http2_windows_and_frame_limits_are_configurable_and_bounded () {

    let config = parse(r#"
        set_upstream("127.0.0.1:3000")
        set_server { h2_adaptive_window = true, h2_stream_window = 262144, h2_connection_window = 1048576, h2_max_frame = 32768, h2_max_header_bytes = 8192 }
    "#).expect("config");

    assert!(config.server.h2_adaptive_window);
    assert_eq!(config.server.h2_stream_window, 262_144);
    assert_eq!(config.server.h2_connection_window, 1_048_576);
    assert_eq!(config.server.h2_max_frame, 32_768);
    assert_eq!(config.server.h2_max_header_bytes, 8_192);

    let defaults = parse("").expect("defaults");

    assert!(!defaults.server.h2_adaptive_window);
    assert_eq!(defaults.server.h2_stream_window, 1_048_576);
    assert_eq!(defaults.server.h2_connection_window, 4_194_304);
    assert_eq!(defaults.server.h2_max_frame, 16_384);
    assert_eq!(defaults.server.h2_max_header_bytes, 65_536);

    assert!(parse(r#"set_server { h2_max_frame = 1024 }"#).is_err());
    assert!(parse(r#"set_server { h2_stream_window = 1000 }"#).is_err());
    assert!(parse(r#"set_server { h2_max_header_bytes = 100 }"#).is_err());

}

#[test]
fn http3_transport_knobs_parse_and_validate () {

    let config = parse(r#"
        set_upstream("127.0.0.1:3000")
        set_tls { cert = "/dev/null", key = "/dev/null" }
        set_http3 { enabled = true, congestion = "bbr", stream_window = 4194304, send_window = 16777216 }
    "#).expect("config");

    assert_eq!(config.http3.congestion, Congestion::Bbr);
    assert_eq!(config.http3.stream_window, 4_194_304);
    assert_eq!(config.http3.send_window, 16_777_216);
    assert_eq!(parse("").expect("defaults").http3.congestion, Congestion::Cubic);

    assert!(parse(r#"set_tls { cert = "/dev/null", key = "/dev/null" } set_http3 { enabled = true, congestion = "fast" }"#).is_err());
    assert!(parse(r#"set_tls { cert = "/dev/null", key = "/dev/null" } set_http3 { enabled = true, stream_window = 10 }"#).is_err());

}

#[test]
fn tls_resumption_knobs_parse_and_validate () {

    let config = parse(r#"
        set_upstream("127.0.0.1:3000")
        set_tls { cert = "/dev/null", key = "/dev/null", session_cache = 1024, tickets = false }
    "#).expect("config");

    let tls = config.tls.expect("tls");

    assert_eq!(tls.session_cache, 1_024);
    assert!(!tls.tickets);

    let defaults = parse(r#"set_tls { cert = "/dev/null", key = "/dev/null" }"#).expect("defaults").tls.expect("tls");

    assert_eq!(defaults.session_cache, 16_384);
    assert!(defaults.tickets);
    assert!(parse(r#"set_tls { cert = "/dev/null", key = "/dev/null", session_cache = 100000000 }"#).is_err());

}
