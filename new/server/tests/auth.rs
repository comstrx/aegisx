mod support;

use std::collections::BTreeMap;
use std::fs;

use aegisx::config::{BasicAuth, Route};
use support::{Http1, Origin, proxy};

fn basic ( users: &[( &str, &str )], file: Option<std::path::PathBuf> ) -> BasicAuth {

    BasicAuth { realm: "area 51".to_string(), users: users.iter().map(|( user, hash )| ( user.to_string(), hash.to_string() )).collect::<BTreeMap<_, _>>(), users_file: file }

}

fn credentials ( user: &str, password: &str ) -> String {

    use base64::Engine;

    format!("Basic {}", base64::engine::general_purpose::STANDARD.encode(format!("{user}:{password}")))

}

#[test]
fn basic_auth_challenges_and_accepts_plain_sha_and_bcrypt_users () {

    let origin = Origin::start();
    let bcrypted = bcrypt::hash("secret", 4).expect("bcrypt");
    let dir = std::env::temp_dir().join(format!("aegisx-auth-{}", std::process::id()));

    fs::create_dir_all(&dir).expect("dir");
    fs::write(dir.join("htpasswd"), "# comment\n\ndave:{PLAIN}pass\n").expect("htpasswd");

    let running = proxy(origin.addr, |config| {
        config.routes.push(Route { name: "private".to_string(), path: "/private".to_string(), upstream: "default".to_string(), basic_auth: Some(basic(&[( "alice", "{PLAIN}secret" ), ( "bob", "{SHA}5en6G6MezRroT3XKqkdPOmY/BfQ=" ), ( "carol", &bcrypted )], Some(dir.join("htpasswd")))), ..Route::default() });
        config.routes.push(Route { name: "public".to_string(), path: "/".to_string(), upstream: "default".to_string(), ..Route::default() });
    });
    let mut client = Http1::connect(running.addr());

    let denied = client.get("/private/x");

    assert_eq!(denied.status, 401);
    assert_eq!(denied.header("www-authenticate"), Some("Basic realm=\"area 51\", charset=\"UTF-8\""));
    assert_eq!(client.request("GET", "/private/x", &[( "Authorization", &credentials("alice", "wrong") )], b"").status, 401);
    assert_eq!(client.request("GET", "/private/x", &[( "Authorization", &credentials("mallory", "secret") )], b"").status, 401);
    assert_eq!(client.request("GET", "/private/x", &[( "Authorization", "Bearer nope" )], b"").status, 401);
    assert_eq!(origin.seen().len(), 0);

    for ( user, password ) in [( "alice", "secret" ), ( "bob", "secret" ), ( "carol", "secret" ), ( "dave", "pass" )] {

        assert_eq!(client.request("GET", "/private/x", &[( "Authorization", &credentials(user, password) )], b"").status, 200, "{user}");
        assert_eq!(client.request("GET", "/private/again", &[( "Authorization", &credentials(user, password) )], b"").status, 200, "{user} cached");

    }

    let seen = origin.seen();

    assert_eq!(seen.len(), 8);
    assert!(seen[0].header("authorization").is_some());
    assert_eq!(client.get("/open").status, 200);

    running.stop().expect("stop");

}

#[test]
fn basic_auth_rejects_unsupported_hashes_and_empty_user_lists () {

    let source = |auth: &str| format!(r#"
        set_upstream("127.0.0.1:3000")
        add_route {{ name = "p", path = "/", basic_auth = {{ {auth} }} }}
    "#);

    assert!(aegisx::config::Config::parse(&source(r#"users = { alice = "$apr1$abc$xyz" }"#), "a.lua", std::path::Path::new("/tmp")).is_err());
    assert!(aegisx::config::Config::parse(&source(r#"realm = "x""#), "b.lua", std::path::Path::new("/tmp")).is_err());
    assert!(aegisx::config::Config::parse(&source(r#"users = { alice = "{PLAIN}pw" }"#), "c.lua", std::path::Path::new("/tmp")).is_ok());

}

fn signed ( head: &str, claims: &str, sign: impl FnOnce(&[u8]) -> Vec<u8> ) -> String {

    use base64::Engine;

    let encode = |bytes: &[u8]| base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes);
    let body = format!("{}.{}", encode(head.as_bytes()), encode(claims.as_bytes()));
    let tag = sign(body.as_bytes());

    format!("{body}.{}", encode(&tag))

}

#[test]
fn bearer_tokens_are_verified_and_claims_become_headers () {

    use aegisx::config::JwtConfig;
    use aws_lc_rs::signature::KeyPair;

    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).expect("clock").as_secs();
    let mac = |secret: &'static [u8]| move |body: &[u8]| aws_lc_rs::hmac::sign(&aws_lc_rs::hmac::Key::new(aws_lc_rs::hmac::HMAC_SHA256, secret), body).as_ref().to_vec();
    let random = aws_lc_rs::rand::SystemRandom::new();
    let pair = aws_lc_rs::signature::EcdsaKeyPair::generate(&aws_lc_rs::signature::ECDSA_P256_SHA256_FIXED_SIGNING).expect("key pair");
    let point = pair.public_key().as_ref().to_vec();
    let dir = std::env::temp_dir().join(format!("aegisx-jwks-{}", std::process::id()));

    std::fs::create_dir_all(&dir).expect("dir");

    {

        use base64::Engine;

        let encode = |bytes: &[u8]| base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes);

        std::fs::write(dir.join("keys.json"), format!(r#"{{"keys":[{{"kty":"EC","crv":"P-256","kid":"k1","x":"{}","y":"{}"}}]}}"#, encode(&point[1..33]), encode(&point[33..]))).expect("jwks");

    }

    let origin = Origin::start();
    let running = proxy(origin.addr, |config| {

        config.jwts.insert("shared".to_string(), JwtConfig { secret: Some("top-secret".to_string()), issuer: Some("issuer.test".to_string()), claims: [( "sub".to_string(), "x-user".to_string() ), ( "level".to_string(), "x-level".to_string() )].into_iter().collect(), ..JwtConfig::default() });
        config.jwts.insert("public".to_string(), JwtConfig { jwks: Some(dir.join("keys.json")), claims: [( "sub".to_string(), "x-user".to_string() )].into_iter().collect(), ..JwtConfig::default() });
        config.routes.push(Route { name: "api".to_string(), path: "/api".to_string(), upstream: "default".to_string(), jwt: Some("shared".to_string()), ..Route::default() });
        config.routes.push(Route { name: "edge".to_string(), path: "/edge".to_string(), upstream: "default".to_string(), jwt: Some("public".to_string()), ..Route::default() });
        config.routes.push(Route { name: "open".to_string(), path: "/".to_string(), upstream: "default".to_string(), ..Route::default() });

    });
    let mut client = Http1::connect(running.addr());
    let mut ask = |path: &str, token: Option<&str>| {

        let auth = token.map(|token| format!("Bearer {token}"));
        let mut headers = vec![( "X-User", "root" )];

        if let Some(auth) = &auth { headers.push(( "Authorization", auth.as_str() )); }

        let reply = client.request("GET", path, &headers, b"");

        ( reply.status, reply.header("www-authenticate").map(str::to_owned) )

    };

    let good = signed(r#"{"alg":"HS256","typ":"JWT"}"#, &format!(r#"{{"sub":"u-42","level":7,"iss":"issuer.test","exp":{}}}"#, now + 60), mac(b"top-secret"));
    let forged = signed(r#"{"alg":"HS256"}"#, &format!(r#"{{"sub":"u-42","iss":"issuer.test","exp":{}}}"#, now + 60), mac(b"other"));
    let expired = signed(r#"{"alg":"HS256"}"#, &format!(r#"{{"sub":"u-42","iss":"issuer.test","exp":{}}}"#, now - 600), mac(b"top-secret"));
    let foreign = signed(r#"{"alg":"HS256"}"#, &format!(r#"{{"sub":"u-42","iss":"elsewhere","exp":{}}}"#, now + 60), mac(b"top-secret"));
    let unsigned = signed(r#"{"alg":"none"}"#, r#"{"sub":"u-42","iss":"issuer.test"}"#, |_| Vec::new());
    let curve = signed(r#"{"alg":"ES256","kid":"k1"}"#, &format!(r#"{{"sub":"edge-9","exp":{}}}"#, now + 60), |body| pair.sign(&random, body).expect("sign").as_ref().to_vec());
    let confused = signed(r#"{"alg":"HS256","kid":"k1"}"#, &format!(r#"{{"sub":"edge-9","exp":{}}}"#, now + 60), |body| aws_lc_rs::hmac::sign(&aws_lc_rs::hmac::Key::new(aws_lc_rs::hmac::HMAC_SHA256, &point), body).as_ref().to_vec());

    assert_eq!(ask("/api/a", None).0, 401);
    assert!(ask("/api/a", None).1.is_some_and(|challenge| challenge.contains("missing token")));
    assert!(ask("/api/a", Some(&forged)).1.is_some_and(|challenge| challenge.contains("signature")));
    assert!(ask("/api/a", Some(&expired)).1.is_some_and(|challenge| challenge.contains("expired")));
    assert!(ask("/api/a", Some(&foreign)).1.is_some_and(|challenge| challenge.contains("issuer")));
    assert!(ask("/api/a", Some(&unsigned)).1.is_some_and(|challenge| challenge.contains("algorithm")));
    assert_eq!(origin.seen().len(), 0);
    assert_eq!(ask("/api/a", Some(&good)).0, 200);
    assert_eq!(ask("/api/b", Some(&good)).0, 200);
    assert_eq!(ask("/edge/a", Some(&curve)).0, 200);
    assert!(ask("/edge/a", Some(&confused)).1.is_some_and(|challenge| challenge.contains("algorithm")));
    assert_eq!(ask("/open", None).0, 200);

    let seen = origin.seen();

    assert_eq!(( seen[0].header("x-user"), seen[0].header("x-level") ), ( Some("u-42"), Some("7") ));
    assert_eq!(seen[1].header("x-user"), Some("u-42"));
    assert_eq!(seen[2].header("x-user"), Some("edge-9"));
    assert_eq!(seen[3].header("x-user"), Some("root"));

    running.stop().expect("stop");

    let _ = std::fs::remove_dir_all(&dir);

    assert!(aegisx::config::Config::parse(r#"set_upstream("127.0.0.1:3000") add_jwt("a", { secret = "x", jwks = "k.json" })"#, "jwt.lua", std::path::Path::new("/tmp")).is_err());
    assert!(aegisx::config::Config::parse(r#"set_upstream("127.0.0.1:3000") add_jwt("a", { secret = "x" }) add_route { name = "r", path = "/", jwt = "b" }"#, "jwt.lua", std::path::Path::new("/tmp")).is_err());
    assert!(aegisx::config::Config::parse(r#"set_upstream("127.0.0.1:3000") add_jwt("a", { secret = "x", algorithms = { "HS256" }, claims = { sub = "x-user" } }) add_route { name = "r", path = "/", jwt = "a" }"#, "jwt.lua", std::path::Path::new("/tmp")).is_ok());

}
