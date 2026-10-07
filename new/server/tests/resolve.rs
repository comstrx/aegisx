mod support;

use std::path::Path;

use aegisx::config::{BackendConfig, Config, PoolConfig};
use aegisx::core::net::Address;
use support::{Http1, Origin, proxy};

#[test]
fn backend_names_resolve_at_boot_and_expand_into_one_backend_per_address () {

    let origin = Origin::start();
    let named = Address::parse(&format!("localhost.:{}", origin.addr.port())).expect("named address");

    assert!(matches!(&named, Address::Name(host, port) if &**host == "localhost" && *port == origin.addr.port()), "{named:?}");

    let running = proxy(origin.addr, |config| {

        let pool = config.pools.entry("default".to_string()).or_default();

        pool.backends = vec![BackendConfig { address: named.clone(), ..BackendConfig::default() }];
        pool.options.attempts = 3;
        pool.options.retry_on = vec!["connect".to_string()];

    });
    let mut client = Http1::connect(running.addr());

    for index in 0..6 { assert_eq!(client.get(&format!("/{index}")).status, 200, "request {index}"); }

    assert_eq!(origin.seen().len(), 6);

    let pools = running.pools();
    let backends: Vec<String> = pools.list[0].backends.iter().map(|backend| backend.addr.to_string()).collect();

    assert!(backends.iter().any(|addr| addr.starts_with("127.0.0.1:")), "{backends:?}");
    assert!(pools.list[0].backends.iter().all(|backend| backend.origin.as_deref() == Some("localhost")), "{backends:?}");

    running.stop().expect("stop");

}

#[test]
fn unresolvable_names_and_bad_names_are_rejected () {

    assert!(Address::parse("api.internal:8080").is_ok());
    assert!(Address::parse("-bad.example:80").is_err());
    assert!(Address::parse("no_port.example").is_err());
    assert_eq!(Address::parse("Api.Example.COM.:443").expect("name").to_string(), "api.example.com:443");

    let config = Config::parse(r#"
        set_upstream("127.0.0.1:3000")
        add_upstream("api", "backend.internal:8080")
        add_upstream("secure", { address = "secure.internal:8443", tls = true })
        set_client { resolve_ms = 5000 }
    "#, "names.lua", Path::new("/tmp")).expect("config");

    assert_eq!(config.pools["api"].backends[0].address, Address::Name("backend.internal".into(), 8080));
    assert!(config.pools["secure"].backends[0].server_name.is_empty());
    assert_eq!(config.client.resolve_ms, 5_000);

    let unresolvable = Config::parse(r#"add_upstream("api", "does-not-exist.invalid:8080")"#, "bad.lua", Path::new("/tmp")).expect("config parses");
    let error = aegisx::app::Boot::check(&unresolvable).err().map(|error| error.to_string()).unwrap_or_default();

    assert!(error.contains("cannot resolve"), "{error}");

    assert!(Config::parse(r#"set_upstream("127.0.0.1:3000") set_client { resolve_ms = 10 }"#, "r.lua", Path::new("/tmp")).is_err());

    let config = PoolConfig::default();

    assert!(config.backends.is_empty());

}

#[test]
fn service_names_are_validated_and_a_missing_service_stops_the_boot () {

    use aegisx::config::{BackendConfig, Config, PoolConfig};

    let with = |service: &str| {

        let mut config = Config::default();

        config.pools.insert("api".to_string(), PoolConfig { backends: vec![BackendConfig { srv: Some(service.to_string()), ..BackendConfig::default() }], ..PoolConfig::default() });
        config.default_pool = Some("api".to_string());

        aegisx::app::Boot::check(&config).map(|_| ()).map_err(|error| error.to_string())

    };

    assert!(with("api.internal").expect_err("not a service name").contains("service name"));
    assert!(with("_http._tcp.aegisx-missing.invalid.").expect_err("no such service").contains("_http._tcp.aegisx-missing.invalid."));

    let parsed = Config::parse(r#"add_upstream("api", { srv = "_http._tcp.api.internal" }) set_default_upstream("api")"#, "srv.lua", std::path::Path::new("/tmp")).expect("config");

    assert_eq!(parsed.pools["api"].backends[0].srv.as_deref(), Some("_http._tcp.api.internal"));

}

