use std::borrow::Cow;
use std::sync::Arc;

use super::{RouteState, Snapshot};

impl Snapshot {

    pub fn route ( &self, host: &str, path: &str, method: &str, headers: &pingora::http::RequestHeader ) -> Option<Arc<RouteState>> {

        if self.routes.is_empty() { return self.fallback.clone(); }
        let host = host.split(':').next().unwrap_or("").trim_end_matches('.');
        self.index.select(path, |index| {
            let route=&self.routes[index];
            route.spec.host.as_ref().is_none_or(|expected| match expected.strip_prefix("*.") {
                Some(suffix) => host.len() > suffix.len() + 1 && host.as_bytes().get(host.len() - suffix.len() - 1) == Some(&b'.')
                    && host[host.len() - suffix.len()..].eq_ignore_ascii_case(suffix),
                None => expected.eq_ignore_ascii_case(host),
            })
                && route.spec.match_headers.iter().all(|(name, value)| {
                    let mut values = headers.headers.get_all(name).iter();
                    values.next().is_some_and(|actual| actual.as_bytes() == value.as_bytes()) && values.next().is_none()
                })
                && (route.spec.methods.is_empty() || route.spec.methods.iter().any(|expected| expected == method))
                && if route.spec.exact { path == route.spec.path } else {
                    path == route.spec.path || (path.starts_with(&route.spec.path)
                        && (route.spec.path.ends_with('/') || path.as_bytes().get(route.spec.path.len()) == Some(&b'/')))
                }
        }).map(|index| self.routes[index].clone()).or_else(|| self.fallback.clone())

    }

}

pub fn canonical_path ( path: &str ) -> Option<Cow<'_, str>> {

    if path=="/" {return Some(Cow::Borrowed(path));}
    if path.contains(['\\', ';']) || path.contains("//") { return None; }
    let normalized = if path.contains('%') {
        let bytes = path.as_bytes();
        let mut value = Vec::with_capacity(bytes.len());
        let mut index = 0;
        while index < bytes.len() {
            if bytes[index] == b'%' {
                let high = (bytes.get(index + 1).copied()? as char).to_digit(16)?;
                let low = (bytes.get(index + 2).copied()? as char).to_digit(16)?;
                let decoded = (high * 16 + low) as u8;
                if matches!(decoded, b'/' | b'\\' | b';' | b'%' | 0..=31 | 127) { return None; }
                if decoded.is_ascii_alphanumeric() || b"-._~".contains(&decoded) { value.push(decoded); }
                else { value.extend_from_slice(&bytes[index..index + 3]); }
                index += 3;
            } else {
                value.push(bytes[index]);
                index += 1;
            }
        }
        Cow::Owned(String::from_utf8(value).ok()?)
    } else { Cow::Borrowed(path) };
    if normalized.split('/').any(|part| matches!(part, "." | "..")) { return None; }

    Some(normalized)

}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ambiguous_paths_cannot_bypass_route_policy () {
        assert_eq!(canonical_path("/%61dmin").as_deref(), Some("/admin"));
        for path in ["/a/../admin", "/a/%2e%2e/admin", "/%2fadmin", "/%252fadmin", "/a\\b", "//admin", "/admin;x", "/bad%"] {
            assert!(canonical_path(path).is_none(), "{path}");
        }
    }
    #[test]
    fn indexed_routes_keep_host_exact_method_and_segment_precedence () {
        use crate::module::config::{Config,Route,PoolConfig,BackendConfig};
        let mut config=Config::default();
        config.pools.insert("default".into(),PoolConfig {backends:vec![BackendConfig::default()],..Default::default()});
        for index in 0..512 {
            config.routes.push(Route {name:format!("route-{index}"),path:format!("/api/{index}"),upstream:"default".into(),..Default::default()});
        }
        config.routes.push(Route {name:"host-priority".into(),host:Some("api.example.test".into()),path:"/".into(),upstream:"default".into(),..Default::default()});
        config.routes.push(Route {name:"exact-priority".into(),path:"/api/500".into(),exact:true,upstream:"default".into(),..Default::default()});
        let snapshot=Snapshot::build(config,None).unwrap();
        let request=pingora::http::RequestHeader::build("GET",b"/",None).unwrap();
        let route=|host,path|snapshot.route(host,path,"GET",&request).unwrap().spec.name.clone();
        assert_eq!(route("other.test","/api/500"),"exact-priority");
        assert_eq!(route("other.test","/api/500/detail"),"route-500");
        assert_eq!(route("other.test","/api/5000"),"default");
        assert_eq!(route("API.EXAMPLE.TEST:8080","/api/500/detail"),"host-priority");
        assert_eq!(route("other.test","/api/511/"),"route-511");
    }

}
