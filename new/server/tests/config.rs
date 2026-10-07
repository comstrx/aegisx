use std::path::Path;

use aegisx::config::{AnalysisMode, Balance, Config};

fn parse ( source: &str ) -> Result<Config, String> {

    Config::parse(source, "test.lua", Path::new("/etc/aegisx")).map_err(|error| error.to_string())

}

#[test]
fn defaults_apply_when_the_file_is_empty () {

    let config = parse("").expect("empty config");

    assert_eq!(config.listen.port(), 8080);
    assert!(config.pools.is_empty());
    assert!(config.routes.is_empty());
    assert_eq!(config.limits.timeout_ms, 10_000);

}

#[test]
fn single_upstream_creates_the_default_pool () {

    let config = parse(r#"
        set_listen("127.0.0.1:9000")
        set_upstream("127.0.0.1:3000")
    "#).expect("config");

    assert_eq!(config.listen.port(), 9000);
    assert_eq!(config.default_pool.as_deref(), Some("default"));
    assert_eq!(config.primary_upstream().expect("upstream").port(), Some(3000));

}

#[test]
fn pools_routes_and_groups_round_trip () {

    let config = parse(r#"
        add_upstream("api", "127.0.0.1:3001")
        add_upstream("api", { address = "127.0.0.1:3002", weight = 3, max_in_flight = 50 })
        set_balancer("api", { policy = "least_conn", max_fails = 3, cooldown_ms = 2000, health = { interval_ms = 500, path = "/ready" } })
        set_default_upstream("api")
        add_route { name = "orders", host = "App.Example.com.", path = "/api/orders", methods = {"post", "get"}, timeout_ms = 2500, strip_prefix = true, request_headers = { ["X-Env"] = "prod" } }
        add_route { name = "private", path = "/internal", deny = true }
        set_limits { timeout_ms = 20000, max_body_bytes = 4096 }
        set_runtime { workers = 3, pin = false }
        set_identity { request_id_header = "x-correlation-id", trusted_peers = {"10.0.0.0/8", "127.0.0.1/32"} }
        set_headers("response", { ["X-Proxy"] = "aegisx" })
        set_tls { cert = "certs/fullchain.pem", key = "/abs/key.pem" }
        set_log { level = "debug", json = true }
    "#).expect("config");

    let api = &config.pools["api"];

    assert_eq!(api.backends.len(), 2);
    assert_eq!(api.backends[1].weight, 3);
    assert_eq!(api.backends[1].max_in_flight, 50);
    assert_eq!(api.options.policy, Balance::LeastConn);
    assert_eq!(api.options.max_fails, 3);
    assert_eq!(api.options.health.as_ref().and_then(|health| health.path.clone()).as_deref(), Some("/ready"));
    assert_eq!(config.default_pool.as_deref(), Some("api"));

    let orders = &config.routes[0];

    assert_eq!(orders.host.as_deref(), Some("app.example.com"));
    assert_eq!(orders.methods, vec!["POST", "GET"]);
    assert_eq!(orders.upstream, "api");
    assert_eq!(orders.timeout_ms, Some(2500));
    assert!(orders.strip_prefix);
    assert_eq!(orders.request_headers.get("x-env").map(String::as_str), Some("prod"));

    assert!(config.routes[1].deny);
    assert_eq!(config.limits.timeout_ms, 20_000);
    assert_eq!(config.limits.max_body_bytes, 4096);
    assert_eq!(config.runtime.workers, 3);
    assert!(!config.runtime.pin);
    assert_eq!(config.identity.request_id_header, "x-correlation-id");
    assert_eq!(config.identity.trusted_peers.len(), 2);
    assert_eq!(config.response_headers.get("x-proxy").map(String::as_str), Some("aegisx"));

    let tls = config.tls.expect("tls");

    assert_eq!(tls.cert, Path::new("/etc/aegisx/certs/fullchain.pem"));
    assert_eq!(tls.key, Path::new("/abs/key.pem"));
    assert_eq!(config.log.level, "debug");
    assert!(config.log.json);

}

#[test]
fn env_helper_reads_the_process_environment () {

    unsafe { std::env::set_var("AEGISX_TEST_UPSTREAM", "127.0.0.1:3456"); }

    let config = parse(r#"set_upstream(env("AEGISX_TEST_UPSTREAM"))"#).expect("config");

    assert_eq!(config.primary_upstream().expect("upstream").port(), Some(3456));

}

#[test]
fn unknown_keys_are_rejected () {

    let error = parse(r#"set_limits { timeout_ms = 1000, bogus = 1 }"#).expect_err("unknown key");

    assert!(error.contains("bogus"), "{error}");

    let error = parse(r#"add_route { name = "x", path = "/", colour = "red" }"#).expect_err("unknown key");

    assert!(error.contains("colour"), "{error}");

}

#[test]
fn bounds_and_references_are_validated () {

    assert!(parse(r#"set_limits { timeout_ms = 10 }"#).expect_err("timeout bound").contains("limits.timeout_ms"));
    assert!(parse(r#"add_upstream("a", { address = "127.0.0.1:1", weight = 0 })"#).expect_err("weight").contains("weight"));
    assert!(parse(r#"add_route { name = "x", path = "/", upstream = "missing" }"#).expect_err("pool").contains("missing"));
    assert!(parse(r#"add_route { name = "x", path = "nope" }"#).expect_err("path").contains("start with /"));
    assert!(parse(r#"set_upstream("127.0.0.1:1") add_route { name = "x", path = "/" } add_route { name = "x", path = "/b" }"#).expect_err("duplicate").contains("twice"));
    assert!(parse(r#"set_default_upstream("ghost")"#).expect_err("default pool").contains("ghost"));
    assert!(parse(r#"set_listen("not-an-address")"#).expect_err("address").contains("set_listen"));
    assert!(parse(r#"add_upstream("tls", { address = "127.0.0.1:443", tls = true })"#).expect_err("server name").contains("server_name"));

}

#[test]
fn sandbox_stops_runaway_scripts () {

    let error = parse("while true do end").expect_err("instruction budget");

    assert!(error.contains("budget"), "{error}");

    let error = parse("os.exit(1)").expect_err("no os library");

    assert!(error.contains("os") || error.contains("nil"), "{error}");

}

#[test]
fn header_tables_reject_duplicate_casings () {

    let error = parse(r#"set_headers("request", { ["X-A"] = "1", ["x-a"] = "2" })"#).expect_err("duplicate header");

    assert!(error.contains("casing"), "{error}");

}

#[test]
fn control_telemetry_and_capture_keys_round_trip () {

    let config = parse(r#"
        set_upstream("127.0.0.1:3000")
        set_control { enabled = true, listen = "127.0.0.1:9191", prefix = "/ops", token_env = "OPS_TOKEN", panel = true, panel_dir = "panel/out", body_bytes = 8192 }
        set_telemetry { recent = 50, journeys = 20 }
        add_route { name = "traced", path = "/jobs", capture = true }
    "#).expect("config");

    assert!(config.control.enabled);
    assert_eq!(config.control.listen.port(), 9191);
    assert_eq!(config.control.prefix, "/ops");
    assert_eq!(config.control.token_env, "OPS_TOKEN");
    assert_eq!(config.control.panel_dir.as_deref(), Some(Path::new("/etc/aegisx/panel/out")));
    assert_eq!(config.control.body_bytes, 8192);
    assert_eq!(config.telemetry.recent, 50);
    assert_eq!(config.telemetry.journeys, 20);
    assert!(config.telemetry.enabled);
    assert!(config.routes[0].capture);

    assert!(parse(r#"set_control { enabled = true, listen = "0.0.0.0:9090" }"#).expect_err("public listen").contains("loopback"));
    assert!(parse(r#"set_control { enabled = true, prefix = "api/" }"#).expect_err("bad prefix").contains("prefix"));
    assert!(parse(r#"set_control { enabled = true, panel = true }"#).expect_err("panel dir").contains("panel_dir"));
    assert!(parse(r#"set_telemetry { recent = 0 }"#).expect_err("recent").contains("telemetry.recent"));
    assert!(parse(r#"set_control { enabled = false, listen = "0.0.0.0:9090" }"#).is_ok());

}

#[test]
fn models_and_analysis_keys_round_trip () {

    let config = parse(r#"
        set_upstream("127.0.0.1:3000")
        add_model("lifecycle", { dir = "weights", features = "weights/features.json", threads = 2 })
        add_model("other", "/opt/models/other")
        set_analysis { mode = "observe", model = "lifecycle", scan_bytes = 2048, workers = 2 }
    "#).expect("config");

    assert_eq!(config.models["lifecycle"].dir, Path::new("/etc/aegisx/weights"));
    assert_eq!(config.models["lifecycle"].features.as_deref(), Some(Path::new("/etc/aegisx/weights/features.json")));
    assert_eq!(config.models["lifecycle"].threads, 2);
    assert_eq!(config.models["other"].dir, Path::new("/opt/models/other"));
    assert_eq!(config.analysis.mode, AnalysisMode::Observe);
    assert_eq!(config.analysis.scan_bytes, 2048);
    assert_eq!(config.analysis.workers, 2);
    assert_eq!(config.analysis.capacity, 256);

    assert!(parse(r#"set_analysis { mode = "observe", model = "missing" }"#).expect_err("missing model").contains("add_model"));
    assert!(parse(r#"set_analysis { mode = "enforce" }"#).expect_err("unknown mode").contains("set_analysis"));
    assert!(parse(r#"add_model("broken", { threads = 0 })"#).expect_err("missing dir").contains("dir"));
    assert!(parse(r#"add_model("broken", { dir = "x", threads = 0 })"#).expect_err("threads").contains("threads"));
    assert!(parse(r#"set_tls { cert = "", key = "k.pem" }"#).expect_err("empty cert").contains("cert"));
    assert!(parse(r#"set_analysis { mode = "off", model = "missing" }"#).is_ok());

}

#[test]
fn rate_limit_keys_round_trip_and_are_bounded () {

    let config = parse(r#"
        set_upstream("127.0.0.1:3000")
        set_limits { rate_limit_10s = 50 }
        add_route { name = "tight", path = "/tight", rate_limit_10s = 2 }
    "#).expect("config");

    assert_eq!(config.limits.rate_limit_10s, 50);
    assert_eq!(config.routes[0].rate_limit_10s, Some(2));
    assert!(parse(r#"set_limits { rate_limit_10s = 2000000 }"#).expect_err("too high").contains("rate_limit_10s"));

}

#[test]
fn decision_keys_round_trip () {

    let config = parse(r#"
        set_upstream("127.0.0.1:3000")
        set_decisions { enabled = true, path = "state/aegisx.db", deny_ttl_ms = 60000, capacity = 500 }
        set_identity { backend_block_header = "x-block" }
        add_route { name = "open", path = "/open", decisions = false }
    "#).expect("config");

    assert!(config.decisions.enabled);
    assert_eq!(config.decisions.path, Path::new("/etc/aegisx/state/aegisx.db"));
    assert_eq!(config.decisions.deny_ttl_ms, 60_000);
    assert_eq!(config.decisions.capacity, 500);
    assert_eq!(config.identity.backend_block_header.as_deref(), Some("x-block"));
    assert_eq!(config.routes[0].decisions, Some(false));
    assert!(parse(r#"set_decisions { enabled = true, deny_ttl_ms = 10 }"#).expect_err("ttl").contains("deny_ttl_ms"));
    assert!(parse(r#"set_identity { backend_block_header = "bad header" }"#).expect_err("header").contains("backend_block_header"));

}

#[test]
fn certificates_and_accept_mode_round_trip () {

    let config = parse(r#"
        set_upstream("127.0.0.1:3000")
        set_runtime { accept = "shared" }
        set_tls { cert = "a.pem", key = "a.key" }
        add_certificate { names = {"b.test", "*.wild.test"}, cert = "b.pem", key = "b.key" }
    "#).expect("config");

    assert_eq!(config.runtime.accept, aegisx::http::server::Accept::Shared);

    let tls = config.tls.expect("tls");

    assert_eq!(tls.certificates.len(), 1);
    assert_eq!(tls.certificates[0].names, vec!["b.test".to_string(), "*.wild.test".to_string()]);
    assert_eq!(tls.certificates[0].cert, Path::new("/etc/aegisx/b.pem"));
    assert!(parse(r#"set_tls { cert = "a.pem", key = "a.key" } add_certificate { names = {"bad*name"}, cert = "b.pem", key = "b.key" }"#).expect_err("name").contains("server name"));
    assert!(parse(r#"set_tls { cert = "a.pem", key = "a.key" } add_certificate { names = {}, cert = "b.pem", key = "b.key" }"#).expect_err("names").contains("names"));

}

#[test]
fn backend_protocol_round_trips () {

    let config = parse(r#"
        add_upstream("grpc", { address = "127.0.0.1:50051", protocol = "http2" })
        add_upstream("grpc", { address = "127.0.0.1:50052" })
        set_default_upstream("grpc")
    "#).expect("config");

    assert_eq!(config.pools["grpc"].backends[0].protocol, aegisx::http::upstream::Protocol::Http2);
    assert_eq!(config.pools["grpc"].backends[1].protocol, aegisx::http::upstream::Protocol::Auto);
    assert!(parse(r#"add_upstream("x", { address = "127.0.0.1:1", protocol = "spdy" })"#).is_err());

}

#[test]
fn lua_configs_can_loop_format_and_include () {

    let dir = std::env::temp_dir().join(format!("aegisx-include-{}", std::process::id()));

    std::fs::create_dir_all(dir.join("conf.d")).expect("dir");
    std::fs::write(dir.join("pools.lua"), r#"for index, port in ipairs({ 3001, 3002, 3003 }) do add_upstream("api", { address = string.format("127.0.0.1:%d", port), weight = index }) end"#).expect("pools");
    std::fs::write(dir.join("conf.d/20-b.lua"), r#"add_route { name = "b", path = "/b", upstream = "api" }"#).expect("b");
    std::fs::write(dir.join("conf.d/10-a.lua"), r#"add_route { name = "a", path = "/a", upstream = "api" }"#).expect("a");
    std::fs::write(dir.join("conf.d/notes.txt"), "not lua").expect("notes");
    std::fs::write(dir.join("loop.lua"), r#"include("loop.lua")"#).expect("loop");

    let config = Config::parse(r#"
        include("pools.lua")
        include("conf.d")

        local tenants = { alpha = "/alpha", beta = "/beta" }
        local names = {}

        for name in pairs(tenants) do table.insert(names, name) end

        table.sort(names)

        for _, name in ipairs(names) do add_route { name = name, path = tenants[name], upstream = "api", rate_per_second = math.max(1, #name) } end
    "#, "main.lua", &dir).expect("config");

    assert_eq!(config.pools["api"].backends.iter().map(|backend| backend.weight).collect::<Vec<_>>(), [1, 2, 3]);
    assert_eq!(config.routes.iter().map(|route| route.name.as_str()).collect::<Vec<_>>(), ["a", "b", "alpha", "beta"]);
    assert_eq!(config.routes[2].rate_per_second, Some(5));

    let nested = Config::parse(r#"include("loop.lua")"#, "main.lua", &dir).expect_err("depth").to_string();

    assert!(nested.contains("deeper"), "{nested}");
    assert!(parse(r#"include("missing.lua")"#).expect_err("missing").contains("missing.lua"));
    assert!(parse("load('return 1')()").is_err());
    assert!(parse("require('os')").is_err());
    assert!(parse("io.open('/etc/passwd')").is_err());

    let many = parse(r#"set_upstream("127.0.0.1:3000") for index = 1, 1000 do add_route { name = "r" .. index, path = "/r" .. tostring(index) } end"#).expect("many routes");

    assert_eq!(many.routes.len(), 1_000);

    let _ = std::fs::remove_dir_all(&dir);

}
