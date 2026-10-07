use aws_lc_rs::hmac;
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use http::Uri;

use crate::config::SecureLink;
use crate::core::env::Env;
use crate::core::error::{AppError, AppResult};
use crate::http::key::HashKey;
use super::arch::Link;

impl Link {

    pub fn compile ( spec: &SecureLink ) -> AppResult<Self> {

        let secret = match ( &spec.secret, &spec.secret_env ) {
            ( Some(secret), None ) => secret.clone(),
            ( None, Some(name) ) => Env::get(name.clone()).filter(|secret| !secret.is_empty()).ok_or_else(|| AppError::config("secure_link", format!("environment variable `{name}` is empty or unset")))?,
            _ => return Err(AppError::config("secure_link", "set secret or secret_env, one of them")),
        };

        if secret.len() < 16 { return Err(AppError::config("secure_link", "the secret needs at least 16 bytes")); }

        Ok(Self { key: hmac::Key::new(hmac::HMAC_SHA256, secret.as_bytes()), signature: spec.signature.as_str().into(), expires: spec.expires.as_str().into() })

    }

    pub fn sign ( &self, path: &str, expires: u64 ) -> String {

        URL_SAFE_NO_PAD.encode(hmac::sign(&self.key, format!("{path}\n{expires}").as_bytes()))

    }

    pub fn verify ( &self, uri: &Uri, now: u64 ) -> Result<(), u16> {

        let expires = HashKey::query(uri, &self.expires).and_then(|value| std::str::from_utf8(value).ok()).and_then(|value| value.parse::<u64>().ok()).ok_or(403u16)?;
        let signature = HashKey::query(uri, &self.signature).and_then(|value| URL_SAFE_NO_PAD.decode(value).ok()).ok_or(403u16)?;

        hmac::verify(&self.key, format!("{}\n{expires}", uri.path()).as_bytes(), &signature).map_err(|_| 403u16)?;

        if now > expires { return Err(410); }

        Ok(())

    }

}
