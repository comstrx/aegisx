use bytes::Bytes;
use pingora::http::{RequestHeader, ResponseHeader};
use crate::module::config::CacheConfig;
use super::Caches;

fn public () -> ResponseHeader {
    let mut response = ResponseHeader::build(200, None).unwrap();
    response.insert_header("cache-control", "public, max-age=60").unwrap();
    response
}

#[test]
fn freshness_privacy_and_duplicate_encoding_are_conservative () {
    let config = CacheConfig::default();
    let cache = Caches::new(&config);
    let key = [1;32];
    let mut response = public();
    response.insert_header("date", "Thu, 01 Jan 1970 00:00:00 GMT").unwrap();
    assert!(cache.fill(key, &response, &config).is_none());
    response.remove_header("date");
    response.insert_header("age", "60").unwrap();
    assert!(cache.fill(key, &response, &config).is_none());
    response.remove_header("age");
    response.insert_header("cache-control", "public, max-age=0, max-age=60").unwrap();
    assert!(cache.fill(key, &response, &config).is_none());
    let mut request = RequestHeader::build("GET", b"/public", None).unwrap();
    request.insert_header("host", "example.test").unwrap();
    request.append_header("accept-encoding", "gzip").unwrap();
    request.append_header("accept-encoding", "br").unwrap();
    assert!(cache.response_key("v1", "route", &request).is_none());
}

#[test]
fn purge_isolates_inflight_fills_and_body_budget_is_enforced () {
    let config = CacheConfig { max_bytes: 1024, max_object_bytes: 1024, ..CacheConfig::default() };
    let cache = Caches::new(&config);
    let mut request = RequestHeader::build("GET", b"/public", None).unwrap();
    request.insert_header("host", "example.test").unwrap();
    let old = cache.response_key("v1", "route", &request).unwrap();
    let mut fill = cache.fill(old, &public(), &config).unwrap();
    assert!(cache.fill([2;32], &public(), &config).is_none());
    assert!(fill.append(Some(&Bytes::from_static(b"ok"))));
    cache.purge_responses();
    cache.save(fill, "x-request-id");
    let new = cache.response_key("v1", "route", &request).unwrap();
    assert_ne!(old, new);
    assert!(cache.responses.get(&new).is_none());
    let mut fill = cache.fill(new, &public(), &config).unwrap();
    assert!(!fill.append(Some(&Bytes::from(vec![0;1025]))));
    assert_ne!(Caches::key(&[b"ab",b"c"]), Caches::key(&[b"a",b"bc"]));
}
