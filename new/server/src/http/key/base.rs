use std::hash::{BuildHasher, Hasher};

use foldhash::fast::FixedState;
use http::header::{COOKIE, HOST, HeaderMap, HeaderName};

use crate::core::error::{AppError, AppResult};
use super::arch::{HashKey, Hint};

const SEED: u64 = 0x5eed_a3a3_9f1d_77c1;

impl HashKey {

    pub fn compile ( spec: Option<&str>, fallback: HashKey ) -> AppResult<Self> {

        let Some(spec) = spec else { return Ok(fallback); };

        match spec.split_once(':') {
            None if spec == "ip" => Ok(Self::Ip),
            None if spec == "uri" => Ok(Self::Uri),
            None if spec == "host" => Ok(Self::Host),
            Some(( "header", name )) => HeaderName::from_bytes(name.trim().as_bytes()).map(Self::Header).map_err(|_| AppError::config("key", format!("invalid header name in key `{spec}`"))),
            Some(( "cookie", name )) if !name.trim().is_empty() => Ok(Self::Cookie(name.trim().into())),
            Some(( "query", name )) if !name.trim().is_empty() => Ok(Self::Query(name.trim().into())),
            _ => Err(AppError::config("key", format!("unsupported key `{spec}`; use ip, uri, host, header:name, cookie:name or query:name"))),
        }

    }

    pub fn digest ( &self, hint: Hint<'_> ) -> u64 {

        let mut hasher = FixedState::with_seed(SEED).build_hasher();

        if self.material(hint, |bytes| hasher.write(bytes)).is_none() { hasher.write(b""); }

        hasher.finish()

    }

    pub fn material <R> ( &self, hint: Hint<'_>, with: impl FnOnce(&[u8]) -> R ) -> Option<R> {

        match self {
            Self::Ip => Some(match hint.ip { std::net::IpAddr::V4(ip) => with(&ip.octets()), std::net::IpAddr::V6(ip) => with(&ip.octets()) }),
            Self::Uri => Some(with(hint.uri.path_and_query().map_or("/", |target| target.as_str()).as_bytes())),
            Self::Host => hint.headers.get(HOST).filter(|value| !value.is_empty()).map(|value| with(value.as_bytes())),
            Self::Header(name) => hint.headers.get(name).filter(|value| !value.is_empty()).map(|value| with(value.as_bytes())),
            Self::Cookie(name) => Self::cookie(hint.headers, name).filter(|value| !value.is_empty()).map(with),
            Self::Query(name) => Self::query(hint.uri, name).filter(|value| !value.is_empty()).map(with),
        }

    }

    pub fn cookie <'h> ( headers: &'h HeaderMap, name: &str ) -> Option<&'h [u8]> {

        headers.get_all(COOKIE).iter()
            .flat_map(|value| value.as_bytes().split(|byte| *byte == b';'))
            .map(<[u8]>::trim_ascii)
            .find_map(|pair| pair.strip_prefix(name.as_bytes()).and_then(|rest| rest.strip_prefix(b"=")))

    }

    pub fn query <'u> ( uri: &'u http::Uri, name: &str ) -> Option<&'u [u8]> {

        uri.query()?.split('&').find_map(|pair| pair.strip_prefix(name)?.strip_prefix('=')).map(str::as_bytes)

    }

}
