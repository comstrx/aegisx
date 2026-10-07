mod support;

use std::path::Path;
use std::thread;
use std::time::Duration;

use aegisx::config::{Config, Route};
use support::{Http1, Origin, proxy};

#[test]
fn a_token_bucket_allows_the_burst_then_paces_the_actor () {

    let origin = Origin::start();
    let running = proxy(origin.addr, |config| {

        config.routes.push(Route { name: "paced".to_string(), path: "/paced".to_string(), upstream: "default".to_string(), rate_per_second: Some(5), rate_burst: Some(2), ..Route::default() });
        config.routes.push(Route { name: "all".to_string(), path: "/".to_string(), upstream: "default".to_string(), ..Route::default() });

    });
    let mut client = Http1::connect(running.addr());

    for _ in 0..3 { assert_eq!(client.get("/paced").status, 200); }

    let limited = client.get("/paced");

    assert_eq!(limited.status, 429);
    assert_eq!(limited.header("retry-after"), Some("1"));
    assert_eq!(client.get("/free").status, 200);

    thread::sleep(Duration::from_millis(450));

    assert_eq!(client.get("/paced").status, 200);
    assert_eq!(origin.seen().len(), 5);

    running.stop().expect("stop");

}

#[test]
fn a_rate_key_gives_every_credential_its_own_bucket () {

    let origin = Origin::start();
    let running = proxy(origin.addr, |config| {

        config.routes.push(Route { name: "api".to_string(), path: "/api".to_string(), upstream: "default".to_string(), rate_per_second: Some(1), rate_burst: Some(1), rate_key: Some("header:x-api-key".to_string()), ..Route::default() });
        config.routes.push(Route { name: "all".to_string(), path: "/".to_string(), upstream: "default".to_string(), ..Route::default() });

    });
    let mut client = Http1::connect(running.addr());
    let mut call = |key: Option<&str>| client.request("GET", "/api", &key.map(|key| ( "X-Api-Key", key )).into_iter().collect::<Vec<_>>(), b"").status;

    assert_eq!([call(Some("alpha")), call(Some("alpha")), call(Some("alpha"))], [200, 200, 429]);
    assert_eq!([call(Some("beta")), call(Some("beta")), call(Some("beta"))], [200, 200, 429]);
    assert_eq!([call(None), call(None), call(None)], [200, 200, 429]);
    assert_eq!(call(Some("alpha")), 429);

    running.stop().expect("stop");

    let bad = Config::parse(r#"set_upstream("127.0.0.1:3000") add_route { name = "api", path = "/api", rate_per_second = 1, rate_key = "body:field" }"#, "rate.lua", Path::new("/tmp")).expect("parse");

    assert!(aegisx::app::Routes::compile(&bad).is_err());

}

#[test]
fn rate_settings_are_parsed_and_bounded () {

    let config = Config::parse(r#"
        set_upstream("127.0.0.1:3000")
        set_limits { rate_per_second = 200, rate_burst = 50 }
        add_route { name = "login", path = "/login", rate_per_second = 2, rate_burst = 5 }
    "#, "rate.lua", Path::new("/tmp")).expect("parse");

    assert_eq!(( config.limits.rate_per_second, config.limits.rate_burst ), ( 200, 50 ));
    assert_eq!(( config.routes[0].rate_per_second, config.routes[0].rate_burst ), ( Some(2), Some(5) ));
    assert!(Config::parse(r#"set_upstream("127.0.0.1:3000") set_limits { rate_per_second = 2000000 }"#, "rate.lua", Path::new("/tmp")).is_err());

}

#[test]
fn a_keyed_concurrency_limit_counts_per_credential () {

    let origin = Origin::start();
    let running = proxy(origin.addr, |config| {

        config.routes.push(Route { name: "jobs".to_string(), path: "/slow".to_string(), upstream: "default".to_string(), concurrency_limit: Some(1), rate_key: Some("header:x-api-key".to_string()), ..Route::default() });
        config.routes.push(Route { name: "all".to_string(), path: "/".to_string(), upstream: "default".to_string(), ..Route::default() });

    });
    let addr = running.addr();
    let call = move |key: &'static str, wait: u64| thread::spawn(move || { thread::sleep(Duration::from_millis(wait)); Http1::connect(addr).request("GET", "/slow/400", &[( "X-Api-Key", key )], b"").status });

    let ( first, second, other ) = ( call("alpha", 0), call("alpha", 120), call("beta", 120) );

    assert_eq!([first.join().expect("first"), second.join().expect("second"), other.join().expect("other")], [200, 503, 200]);

    running.stop().expect("stop");

}

#[test]
fn every_rate_rule_must_pass_and_a_rejection_charges_no_bucket () {

    use aegisx::config::RateRule;

    let origin = Origin::start();
    let running = proxy(origin.addr, |config| {

        let rules = vec![RateRule { key: "header:x-user".to_string(), rate: 1, burst: 0 }, RateRule { key: "header:x-tenant".to_string(), rate: 1, burst: 2 }];

        config.routes.push(Route { name: "api".to_string(), path: "/api".to_string(), upstream: "default".to_string(), rate_rules: rules, ..Route::default() });
        config.routes.push(Route { name: "all".to_string(), path: "/".to_string(), upstream: "default".to_string(), ..Route::default() });

    });
    let mut client = Http1::connect(running.addr());
    let mut call = |user: &str, tenant: Option<&str>| client.request("GET", "/api", &[( "X-User", user )].into_iter().chain(tenant.map(|tenant| ( "X-Tenant", tenant ))).collect::<Vec<_>>(), b"");

    assert_eq!(call("ali", Some("north")).status, 200);

    let again = call("ali", Some("north"));

    assert_eq!(again.status, 429);
    assert_eq!(again.header("retry-after"), Some("1"));
    assert_eq!([call("badr", Some("north")).status, call("core", Some("north")).status], [200, 200], "the user rejection left the tenant bucket alone");
    assert_eq!(call("dina", Some("north")).status, 429, "the tenant bucket is spent");
    assert_eq!(call("dina", Some("south")).status, 200, "the tenant rejection left the user bucket alone");
    assert_eq!([call("emad", None).status, call("emad", None).status], [200, 429], "a rule without its key is skipped");
    assert_eq!(client.get("/free").status, 200);

    running.stop().expect("stop");

}

#[test]
fn rate_rules_fall_back_to_the_global_list_and_are_validated () {

    let origin = Origin::start();
    let running = proxy(origin.addr, |config| config.limits.rate_rules = vec![aegisx::config::RateRule { key: "header:x-tenant".to_string(), rate: 1, burst: 0 }]);
    let mut client = Http1::connect(running.addr());
    let mut call = |path: &str| client.request("GET", path, &[( "X-Tenant", "north" )], b"").status;

    assert_eq!([call("/a"), call("/b")], [200, 429], "a global rule keeps one bucket across routes");

    running.stop().expect("stop");

    let parse = |rules: &str| Config::parse(&format!("set_upstream(\"127.0.0.1:3000\") add_route {{ name = \"api\", path = \"/api\", rate_rules = {{ {rules} }} }}"), "rate.lua", Path::new("/tmp"));

    assert!(parse("{ key = \"ip\", rate = 10, burst = 20 }, { key = \"header:x-api-key\", rate = 100 }").is_ok());
    assert!(parse("{ key = \"ip\", rate = 0 }").is_err());
    assert!(parse("{ key = \"body:field\", rate = 5 }").is_err());
    assert!(parse(&vec!["{ rate = 5 }"; 9].join(", ")).is_err());

}

