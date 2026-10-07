use std::future::{Ready, ready};
use std::time::{SystemTime, UNIX_EPOCH};

use actix_web::dev::Payload;
use actix_web::http::StatusCode;
use actix_web::http::header::AUTHORIZATION;
use actix_web::{FromRequest, HttpRequest, HttpResponse, ResponseError, web};
use hmac::{Hmac, Mac};
use sha2::{Digest, Sha256};

use crate::arch::{ApiError, Session, Settings, State, TOKEN_TTL, Tenant};

impl Settings {

    pub fn load () -> Self {

        let read = |name: &str, fallback: &str| std::env::var(name).unwrap_or_else(|_| fallback.to_string());

        Self {
            listen   : read("APP_LISTEN", "0.0.0.0:3800").parse().unwrap_or_else(|_| ( [0, 0, 0, 0], 3800 ).into()),
            database : read("DATABASE_URL", "postgres://bench:bench@127.0.0.1/bench"),
            workers  : read("APP_WORKERS", "2").parse().unwrap_or(2),
            pool     : read("APP_POOL", "16").parse().unwrap_or(16),
            secret   : read("APP_SECRET", "bench-secret-not-for-production").into_bytes(),
            tokens   : std::env::args().skip_while(|flag| flag != "--tokens").nth(1).and_then(|count| count.parse().ok()).unwrap_or(0),
        }

    }

}

impl State {

    pub fn now () -> u64 {

        SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |since| since.as_secs())

    }

    pub fn digest ( slug: &str, password: &str ) -> String {

        hex::encode(Sha256::digest(format!("{slug}:{password}")))

    }

    pub fn issue ( &self, user: i64, tenant: i32, expires: u64 ) -> String {

        let claims = format!("{user}.{tenant}.{expires}");

        format!("{claims}.{}", hex::encode(self.seal(&claims)))

    }

    pub fn verify ( &self, token: &str ) -> Option<Session> {

        let ( claims, seal ) = token.rsplit_once('.')?;
        let mut mac = Hmac::<Sha256>::new_from_slice(&self.secret).ok()?;

        mac.update(claims.as_bytes());
        mac.verify_slice(&hex::decode(seal).ok()?).ok()?;

        let mut parts = claims.split('.');
        let ( user, tenant, expires ) = ( parts.next()?.parse().ok()?, parts.next()?.parse().ok()?, parts.next()?.parse::<u64>().ok()? );

        (expires > Self::now()).then_some(Session { user, tenant })

    }

    pub fn lifetime () -> u64 {

        Self::now() + TOKEN_TTL

    }

    fn seal ( &self, claims: &str ) -> Vec<u8> {

        let Ok(mut mac) = Hmac::<Sha256>::new_from_slice(&self.secret) else { return Vec::new(); };

        mac.update(claims.as_bytes());

        mac.finalize().into_bytes().to_vec()

    }

}

impl FromRequest for Tenant {

    type Error = ApiError;
    type Future = Ready<Result<Self, ApiError>>;

    fn from_request ( request: &HttpRequest, _: &mut Payload ) -> Self::Future {

        let slug = request.headers().get("x-tenant").and_then(|value| value.to_str().ok()).unwrap_or("t1");

        ready(request.app_data::<web::Data<State>>().and_then(|state| state.tenants.get(slug).copied()).map(Tenant).ok_or(ApiError::Tenant))

    }

}

impl FromRequest for Session {

    type Error = ApiError;
    type Future = Ready<Result<Self, ApiError>>;

    fn from_request ( request: &HttpRequest, _: &mut Payload ) -> Self::Future {

        let token = request.headers().get(AUTHORIZATION).and_then(|value| value.to_str().ok()).and_then(|value| value.strip_prefix("Bearer "));

        ready(request.app_data::<web::Data<State>>().zip(token).and_then(|( state, token )| state.verify(token)).ok_or(ApiError::Unauthorized))

    }

}

impl std::fmt::Display for ApiError {

    fn fmt ( &self, out: &mut std::fmt::Formatter<'_> ) -> std::fmt::Result {

        match self {
            Self::Tenant => out.write_str("unknown tenant"),
            Self::Unauthorized => out.write_str("unauthorized"),
            Self::NotFound => out.write_str("not found"),
            Self::Invalid(reason) | Self::Conflict(reason) => out.write_str(reason),
            Self::Flaky => out.write_str("injected failure"),
            Self::Database(error) => write!(out, "database: {error}"),
        }

    }

}

impl ResponseError for ApiError {

    fn status_code ( &self ) -> StatusCode {

        match self {
            Self::Tenant | Self::Invalid(_) => StatusCode::BAD_REQUEST,
            Self::Unauthorized => StatusCode::UNAUTHORIZED,
            Self::NotFound => StatusCode::NOT_FOUND,
            Self::Conflict(_) => StatusCode::CONFLICT,
            Self::Flaky => StatusCode::BAD_GATEWAY,
            Self::Database(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }

    }

    fn error_response ( &self ) -> HttpResponse {

        HttpResponse::build(self.status_code()).json(serde_json::json!({ "error": self.to_string() }))

    }

}
