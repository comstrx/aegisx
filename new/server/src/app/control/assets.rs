use std::collections::HashMap;
use std::path::Path;

use bytes::Bytes;
use http::header::HeaderValue;

use crate::config::base::consts::PANEL_BYTES_MAX;
use crate::core::error::{AppError, AppFail, AppResult};
use crate::http::body::Body;
use crate::http::response::{Res, Response};
use super::arch::{Admin, Asset, Panel};

const POLICY: &str = "default-src 'self'; script-src 'self' 'unsafe-inline'; style-src 'self' 'unsafe-inline'; connect-src 'self'; object-src 'none'; base-uri 'none'; frame-ancestors 'none'";

impl Panel {

    pub fn load ( dir: &Path ) -> AppResult<Self> {

        if !dir.is_dir() { return Err(AppError::config("set_control", format!("panel_dir {} is not a directory", dir.display()))); }

        let mut files = HashMap::new();
        let mut total = 0u64;
        let mut pending = vec![dir.to_path_buf()];

        while let Some(current) = pending.pop() {

            let entries = std::fs::read_dir(&current).or_fail_with(|| format!("cannot read {}", current.display()))?;

            for entry in entries {

                let path = entry.or_fail("cannot read panel entry")?.path();

                if path.is_dir() { pending.push(path); continue; }

                let relative = path.strip_prefix(dir).or_fail("panel path escapes its directory")?;
                let key = format!("/{}", relative.to_string_lossy().replace('\\', "/"));
                let bytes = std::fs::read(&path).or_fail_with(|| format!("cannot read {}", path.display()))?;

                total += bytes.len() as u64;

                if total > PANEL_BYTES_MAX { return Err(AppError::config("set_control", format!("panel_dir exceeds {PANEL_BYTES_MAX} bytes"))); }

                files.insert(key, Asset { bytes: Bytes::from(bytes), mime: Self::mime(&path) });

            }

        }

        if !files.contains_key("/index.html") { return Err(AppError::config("set_control", format!("panel_dir {} has no index.html", dir.display()))); }

        Ok(Self { files })

    }

    pub fn get ( &self, path: &str ) -> Option<&Asset> {

        if path.contains("..") || path.contains('\\') || path.contains('\0') { return None; }

        let path = if path == "/" { "/index.html" } else { path };

        self.files.get(path).or_else(|| self.files.get(&format!("{path}.html")))

    }

    pub fn respond ( path: &str, asset: &Asset ) -> Res<Body> {

        let mut response = Response::bytes(200, asset.mime, asset.bytes.clone());
        let headers = response.headers_mut();
        let caching = if path.starts_with("/_next/static/") { "public, max-age=31536000, immutable" } else { "no-store" };

        headers.insert("cache-control", HeaderValue::from_static(caching));
        headers.insert("x-content-type-options", HeaderValue::from_static("nosniff"));
        headers.insert("referrer-policy", HeaderValue::from_static("no-referrer"));
        headers.insert("content-security-policy", HeaderValue::from_static(POLICY));

        response

    }

    fn mime ( path: &Path ) -> &'static str {

        match path.extension().and_then(|value| value.to_str()).unwrap_or("") {
            "html" => "text/html; charset=utf-8",
            "js" | "mjs" => "text/javascript; charset=utf-8",
            "css" => "text/css; charset=utf-8",
            "json" | "webmanifest" => "application/json",
            "svg" => "image/svg+xml",
            "png" => "image/png",
            "jpg" | "jpeg" => "image/jpeg",
            "ico" => "image/x-icon",
            "woff2" => "font/woff2",
            "woff" => "font/woff",
            "txt" | "map" => "text/plain; charset=utf-8",
            _ => "application/octet-stream",
        }

    }

}

impl Admin {

    pub fn panel_loaded ( &self ) -> bool {

        self.panel.is_some()

    }

}
