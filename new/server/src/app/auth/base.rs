use std::collections::HashMap;

use aws_lc_rs::constant_time::verify_slices_are_equal;
use aws_lc_rs::digest::{SHA1_FOR_LEGACY_USE_ONLY, SHA256, digest};
use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use foldhash::fast::RandomState;
use http::header::{AUTHORIZATION, HeaderMap, HeaderValue};

use crate::config::BasicAuth;
use crate::core::cache::Cache;
use crate::core::error::{AppError, AppResult};
use super::arch::{Basic, Credential};

const RECENT_CAPACITY: usize = 4_096;
const RECENT_TTL_MS: u64 = 60_000;

impl Basic {

    pub fn compile ( spec: &BasicAuth ) -> AppResult<Self> {

        let realm = HeaderValue::from_str(&format!("Basic realm=\"{}\", charset=\"UTF-8\"", spec.realm.replace(['"', '\\'], ""))).map_err(|_| AppError::config("basic_auth", "realm is not a valid header value"))?;
        let mut users = HashMap::with_capacity_and_hasher(spec.users.len(), RandomState::default());

        for ( user, hash ) in &spec.users { users.insert(user.as_str().into(), Credential::parse(hash)?); }

        if let Some(path) = &spec.users_file {

            let text = std::fs::read_to_string(path).map_err(|error| AppError::config("basic_auth", format!("cannot read {}: {error}", path.display())))?;

            for line in text.lines().map(str::trim).filter(|line| !line.is_empty() && !line.starts_with('#')) {

                let Some(( user, hash )) = line.split_once(':') else { return Err(AppError::config("basic_auth", format!("{}: expected user:hash, found `{line}`", path.display()))); };

                users.insert(user.into(), Credential::parse(hash)?);

            }

        }

        if users.is_empty() { return Err(AppError::config("basic_auth", "no users configured")); }

        Ok(Self { realm, users, recent: Cache::new(RECENT_CAPACITY, RECENT_TTL_MS) })

    }

    pub fn challenge ( &self ) -> HeaderValue {

        self.realm.clone()

    }

    pub async fn allows ( &self, headers: &HeaderMap ) -> bool {

        let Some(value) = headers.get(AUTHORIZATION) else { return false; };
        let bytes = value.as_bytes();

        if bytes.len() < 7 || !bytes[..6].eq_ignore_ascii_case(b"basic ") { return false; }

        let mut key = [0u8; 32];

        key.copy_from_slice(digest(&SHA256, bytes).as_ref());

        if self.recent.get(&key).is_some() { return true; }

        let Ok(decoded) = STANDARD.decode(bytes[6..].trim_ascii()) else { return false; };
        let Ok(text) = std::str::from_utf8(&decoded) else { return false; };
        let Some(( user, password )) = text.split_once(':') else { return false; };
        let Some(credential) = self.users.get(user) else { return false; };

        let accepted = match credential {
            Credential::Plain(expected) => verify_slices_are_equal(expected.as_bytes(), password.as_bytes()).is_ok(),
            Credential::Sha1(expected) => verify_slices_are_equal(digest(&SHA1_FOR_LEGACY_USE_ONLY, password.as_bytes()).as_ref(), expected).is_ok(),
            Credential::Bcrypt(hash) => {

                let ( hash, password ) = ( hash.to_string(), password.to_string() );

                tokio::task::spawn_blocking(move || bcrypt::verify(password, &hash).unwrap_or(false)).await.unwrap_or(false)

            }
        };

        if accepted { self.recent.put(key, ()); }

        accepted

    }

}

impl Credential {

    pub fn parse ( text: &str ) -> AppResult<Self> {

        if let Some(encoded) = text.strip_prefix("{SHA}") {

            let decoded = STANDARD.decode(encoded.trim()).map_err(|_| AppError::config("basic_auth", "invalid {SHA} password hash"))?;
            let bytes: [u8; 20] = decoded.as_slice().try_into().map_err(|_| AppError::config("basic_auth", "invalid {SHA} password hash length"))?;

            return Ok(Self::Sha1(bytes));

        }

        if let Some(plain) = text.strip_prefix("{PLAIN}") { return Ok(Self::Plain(plain.into())); }

        if text.starts_with("$2a$") || text.starts_with("$2b$") || text.starts_with("$2y$") { return Ok(Self::Bcrypt(text.into())); }

        Err(AppError::config("basic_auth", "unsupported password hash: use bcrypt (htpasswd -B), {SHA} or {PLAIN}"))

    }

}
