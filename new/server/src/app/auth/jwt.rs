use http::header::{AUTHORIZATION, HeaderMap, HeaderName, HeaderValue};
use serde_json::Value;

use crate::config::JwtConfig;
use crate::core::cache::Cache;
use crate::core::error::{AppError, AppFail, AppResult};
use crate::core::jwt::{Algorithm, Fault, Verifier};
use crate::http::key::HashKey;
use super::arch::{Bearer, Grant};

const RECENT_CAPACITY: usize = 16_384;
const RECENT_TTL_MS: u64 = 300_000;
const TOKEN_BYTES: usize = 8_192;

impl Bearer {

    pub fn compile ( spec: &JwtConfig ) -> AppResult<Self> {

        let keys = match ( spec.secret.as_deref().filter(|secret| !secret.is_empty()), &spec.jwks ) {
            ( Some(secret), _ ) => vec![Verifier::secret(secret.as_bytes())],
            ( None, Some(file) ) => Verifier::jwks(&std::fs::read(file).or_fail_with(|| format!("cannot read key set {}", file.display()))?)?,
            ( None, None ) => Vec::new(),
        };

        let allowed = spec.algorithms.iter().filter_map(|name| Algorithm::named(name)).collect();
        let name = |text: &str| HeaderName::from_bytes(text.to_ascii_lowercase().as_bytes()).map_err(|_| AppError::config("add_jwt", format!("`{text}` is not a header name")));
        let header = spec.header.as_deref().map(name).transpose()?;

        Ok(Self {
            verifier : Verifier::new(keys, allowed, spec.issuer.as_deref(), spec.audience.as_deref(), spec.leeway_s)?,
            scheme   : header.is_none(),
            header   : header.unwrap_or(AUTHORIZATION),
            cookie   : spec.cookie.as_deref().filter(|cookie| !cookie.is_empty()).map(Into::into),
            claims   : spec.claims.iter().map(|( claim, target )| Ok(( claim.as_str().into(), name(target)? ))).collect::<AppResult<Vec<_>>>()?,
            recent   : Cache::new(RECENT_CAPACITY, RECENT_TTL_MS),
        })

    }

    pub fn names ( &self ) -> impl Iterator<Item = &HeaderName> {

        self.claims.iter().map(|( _, name )| name)

    }

    pub fn admit ( &self, headers: &HeaderMap, now_ms: u64 ) -> Result<Grant, Fault> {

        let presented = headers.get(&self.header).map(|value| value.as_bytes()).and_then(|value| match self.scheme {
            true => value.get(..7).filter(|scheme| scheme.eq_ignore_ascii_case(b"bearer ")).map(|_| value[7..].trim_ascii()),
            false => Some(value.trim_ascii()),
        });

        let token = presented.or_else(|| self.cookie.as_deref().and_then(|cookie| HashKey::cookie(headers, cookie))).filter(|token| !token.is_empty() && token.len() <= TOKEN_BYTES).ok_or(Fault::Missing)?;

        if let Some(grant) = self.recent.get(&Box::from(token)) { return Ok(grant); }

        let claims = self.verifier.verify(token, now_ms / 1_000)?;

        let grant: Grant = self.claims.iter().filter_map(|( claim, name )| {

            let text = match claims.get(&**claim)? {
                Value::String(text) => text.clone(),
                Value::Number(number) => number.to_string(),
                Value::Bool(flag) => flag.to_string(),
                _ => return None,
            };

            HeaderValue::from_str(&text).ok().map(|value| ( name.clone(), value ))

        }).collect();

        let left_ms = claims.get("exp").and_then(Value::as_u64).map_or(RECENT_TTL_MS, |expires| expires.saturating_mul(1_000).saturating_sub(now_ms));

        if left_ms > 0 { self.recent.put_for(token.into(), grant.clone(), left_ms.min(RECENT_TTL_MS)); }

        Ok(grant)

    }

    pub fn challenge ( fault: Fault ) -> HeaderValue {

        HeaderValue::from_str(&format!("Bearer error=\"invalid_token\", error_description=\"{}\"", fault.reason())).unwrap_or_else(|_| HeaderValue::from_static("Bearer"))

    }

}
