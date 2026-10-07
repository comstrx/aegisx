use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime};
use bytes::Bytes;
use pingora::http::{RequestHeader, ResponseHeader};

use crate::module::config::CacheConfig;
use super::{CachedResponse, Caches, Fill, Key};

impl Caches {

    pub fn response_key ( &self, version: &str, route: &str, request: &RequestHeader ) -> Option<Key> {

        if request.method.as_str() != "GET" || request.headers.contains_key("authorization")
            || request.headers.contains_key("cookie") || request.headers.contains_key("range")
            || request.headers.contains_key("cache-control") || request.headers.contains_key("pragma")
            || request.headers.keys().any(|key| key.as_str().starts_with("if-"))
            || request.headers.contains_key("transfer-encoding")
            || request.headers.get("content-length").is_some_and(|value| value.as_bytes() != b"0")
        { return None; }
        for name in ["host", "accept-encoding"] {
            if request.headers.get_all(name).iter().count() > 1 { return None; }
        }
        let host = request.headers.get("host")?.as_bytes();
        let uri = request.uri.to_string();
        let encoding = request.headers.get("accept-encoding").map_or(&b""[..], |value| value.as_bytes());

        let generation = self.response_generation.load(std::sync::atomic::Ordering::Acquire).to_le_bytes();
        Some(Self::key(&[&generation, version.as_bytes(), route.as_bytes(), host, uri.as_bytes(), encoding]))

    }

    pub fn fill ( &self, key: Key, response: &ResponseHeader, config: &CacheConfig ) -> Option<Fill> {

        if response.status.as_u16() != 200 || response.headers.contains_key("set-cookie")
            || response.headers.contains_key("content-range") || response.headers.contains_key("www-authenticate")
        { return None; }
        if response.headers.get_all("vary").iter().any(|value| value.to_str().ok().is_none_or(|value|
            value.split(',').any(|name| !name.trim().eq_ignore_ascii_case("accept-encoding"))))
        { return None; }
        let mut public = false;
        let mut max_age = None;
        let mut shared_age = None;
        for value in response.headers.get_all("cache-control") {
            for part in value.to_str().ok()?.split(',') {
                let mut directive = part.trim().splitn(2, '=');
                let name = directive.next()?.trim().to_ascii_lowercase();
                let value = directive.next().map(|value| value.trim().trim_matches('"'));
                match name.as_str() {
                    "private" | "no-store" | "no-cache" => return None,
                    "public" => public = true,
                    "s-maxage" => { let age = value?.parse::<u64>().ok()?; shared_age = Some(shared_age.map_or(age, |old: u64| old.min(age))); },
                    "max-age" => { let age = value?.parse::<u64>().ok()?; max_age = Some(max_age.map_or(age, |old: u64| old.min(age))); },
                    _ => {}
                }
            }
        }
        if !public { return None; }
        let age = response.headers.get("age").map(|value| value.to_str().ok()?.parse::<u64>().ok()).unwrap_or(Some(0))?;
        let apparent = response.headers.get("date").map(|value| {
            let date = httpdate::parse_http_date(value.to_str().ok()?).ok()?;
            Some(SystemTime::now().duration_since(date).unwrap_or_default().as_secs())
        }).unwrap_or(Some(0))?;
        let age = age.max(apparent);
        let remaining = shared_age.or(max_age)?.checked_sub(age)?;
        let ttl = Duration::from_secs(remaining).min(Duration::from_millis(config.response_ttl_ms));
        if ttl.is_zero() { return None; }
        let length = response.headers.get("content-length").and_then(|value| value.to_str().ok()?.parse::<usize>().ok());
        if length.is_some_and(|length| length > config.max_object_bytes) { return None; }
        let header_bytes: usize = response.headers.iter().map(|(name, value)| name.as_str().len() + value.len()).sum();
        if header_bytes > 16384 { return None; }
        let permit = self.fills.clone().try_acquire_many_owned(config.max_object_bytes as u32).ok()?;
        let created = Instant::now();

        Some(Fill { key, header: response.clone(), bytes: Vec::with_capacity(config.max_object_bytes), limit: config.max_object_bytes,
            created, expires: created + ttl, initial_age: age, _permit: permit })

    }

    pub fn save ( &self, mut fill: Fill, id_header: &str ) {

        let connection = fill.header.headers.get("connection").and_then(|value| value.to_str().ok())
            .map(|value| value.split(',').map(|name| name.trim().to_owned()).collect::<Vec<_>>()).unwrap_or_default();
        for name in connection { fill.header.remove_header(&name); }
        for name in ["connection", "transfer-encoding", "keep-alive", "upgrade", "trailer", "proxy-authenticate", "x-request-id", id_header] {
            fill.header.remove_header(name);
        }
        let _ = fill.header.insert_header("content-length", fill.bytes.len().to_string());
        let headers: usize = fill.header.headers.iter().map(|(name, value)| name.as_str().len() + value.len()).sum();
        let weight = (fill.bytes.len() + headers + 512) as u32;
        self.responses.insert(fill.key, Arc::new(CachedResponse {
            header: fill.header, body: Bytes::from(fill.bytes.into_boxed_slice()), created: fill.created,
            expires: fill.expires, initial_age: fill.initial_age, weight,
        }));

    }

}

impl Fill {

    pub fn append ( &mut self, body: Option<&Bytes> ) -> bool {

        let Some(body) = body else { return true; };
        if self.bytes.len().saturating_add(body.len()) > self.limit { return false; }
        self.bytes.extend_from_slice(body);
        true

    }

}
