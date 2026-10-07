use std::net::SocketAddr;
use std::rc::Rc;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use http::Method;
use http::header::{HOST, ORIGIN, HeaderMap, HeaderValue};
use http_body_util::BodyExt;
use serde_json::{Value, json};

use crate::app::{Analyser, State, Telemetry};
use crate::config::Config;
use crate::config::base::consts::{TOKEN_BYTES_MAX, TOKEN_BYTES_MIN};
use crate::core::error::{AppError, AppResult};
use crate::core::parse::Json;
use crate::core::rt::{Rt, Workers};
use crate::core::secret::Secret;
use crate::core::sync::Watch;
use crate::core::time::Clock;
use crate::http::body::Body;
use crate::http::request::Req;
use crate::http::response::{Res, Response};
use crate::http::server::{Accept, Listener, Server, Settings};
use super::arch::{Admin, Control, Grant, Panel};

impl Control {

    pub fn prepare ( config: &Config, state: State, telemetry: Arc<Telemetry>, analyser: Option<Arc<Analyser>> ) -> AppResult<Option<Arc<Admin>>> {

        let settings = &config.control;

        if !settings.enabled { return Ok(None); }

        let admin = Secret::from_env(&settings.token_env, TOKEN_BYTES_MIN, TOKEN_BYTES_MAX)?;
        let backend = settings.backend_token_env.as_deref().map(|name| Secret::from_env(name, TOKEN_BYTES_MIN, TOKEN_BYTES_MAX)).transpose()?;

        if backend.as_ref().is_some_and(|backend| backend.same(&admin)) { return Err(AppError::config("set_control", "backend and admin tokens must differ")); }

        let panel = match ( settings.panel, &settings.panel_dir ) {
            ( true, Some(dir) ) => Some(Panel::load(dir)?),
            _ => None,
        };

        Ok(Some(Arc::new(Admin {
            state,
            telemetry,
            analyser,
            settings  : settings.clone(),
            admin,
            backend,
            panel,
            started   : Instant::now(),
            boot_ms   : Clock::now_ms(),
            resources : Mutex::new(None),
        })))

    }

    pub fn start ( admin: Arc<Admin>, server: Settings, stop: Watch ) -> AppResult<Workers> {

        let listener = Listener::bind(admin.settings.listen, 128, 1, Accept::Shared)?.pop().ok_or_else(|| AppError::message("control listener missing"))?;

        Rt::thread("aegisx-control", move || async move {

            let connect = |peer: SocketAddr, _: bool| peer;
            let handler = move |request: Box<Req<Body>>, _: Rc<SocketAddr>| {

                let admin = admin.clone();

                async move { admin.handle(*request).await }

            };

            Server::new(server).serve(listener, stop, connect, handler).await

        })

    }

}

impl Admin {

    pub async fn handle ( self: Arc<Self>, request: Req<Body> ) -> Res<Body> {

        let host = request.headers().get(HOST).and_then(|value| value.to_str().ok()).unwrap_or("").to_owned();

        if !self.host_allowed(&host) { return Self::json(400, &json!({ "error": "invalid_control_host" })); }

        if let Some(origin) = request.headers().get(ORIGIN) && origin.as_bytes() != format!("http://{host}").as_bytes() {

            return Self::json(403, &json!({ "error": "cross_origin_denied" }));

        }

        let path = request.uri().path().to_owned();

        if let Some(rest) = path.strip_prefix(self.settings.prefix.as_str()) && rest.starts_with('/') {

            let grant = if rest.starts_with("/backend/") { Grant::Backend } else { Grant::Admin };

            if !self.authorized(request.headers(), grant) { return Self::json(401, &json!({ "error": "unauthorized" })); }

            let method = request.method().clone();

            let body = match self.read(request).await {
                Ok(body) => body,
                Err(status) => return Self::json(status, &json!({ "error": "invalid_body" })),
            };

            return self.api(&method, rest, &body).await;

        }

        if request.method() != Method::GET || self.panel.is_none() { return Self::json(404, &json!({ "error": "not_found" })); }

        if path == "/aegisx-bootstrap.json" { return Self::json(200, &json!({ "api_prefix": self.settings.prefix, "schema_version": 1 })); }

        match self.panel.as_ref().and_then(|panel| panel.get(&path)) {
            Some(asset) => Panel::respond(&path, asset),
            None => Self::json(404, &json!({ "error": "not_found" })),
        }

    }

    fn host_allowed ( &self, host: &str ) -> bool {

        let listen = self.settings.listen;

        host == listen.to_string() || host == format!("localhost:{}", listen.port())

    }

    fn authorized ( &self, headers: &HeaderMap, grant: Grant ) -> bool {

        let presented = headers.get("authorization").and_then(|value| value.to_str().ok()).and_then(|value| value.strip_prefix("Bearer ")).unwrap_or("");

        match grant {
            Grant::Admin => self.admin.matches(presented.as_bytes()),
            Grant::Backend => self.backend.as_ref().is_some_and(|secret| secret.matches(presented.as_bytes())),
        }

    }

    async fn read ( &self, request: Req<Body> ) -> Result<bytes::Bytes, u16> {

        let body = Body::limited(request.into_body(), self.settings.body_bytes, 10_000);

        match body.collect().await {
            Ok(collected) => Ok(collected.to_bytes()),
            Err(error) => Err(error.status()),
        }

    }

    pub(super) fn json ( status: u16, value: &Value ) -> Res<Body> {

        let payload = Json::to_vec(value).unwrap_or_else(|_| b"{}".to_vec());
        let mut response = Response::bytes(status, "application/json", payload);

        Self::harden(response.headers_mut());

        response

    }

    pub(super) fn harden ( headers: &mut HeaderMap ) {

        headers.insert("cache-control", HeaderValue::from_static("no-store"));
        headers.insert("x-content-type-options", HeaderValue::from_static("nosniff"));

    }

}
