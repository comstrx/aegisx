use std::collections::VecDeque;
use std::fs::Metadata;
use std::io::{ErrorKind, SeekFrom};
use std::path::{Component, Path, PathBuf};
use std::time::UNIX_EPOCH;

use bytes::Bytes;

use http::header::{ACCEPT_RANGES, ALLOW, CACHE_CONTROL, CONTENT_ENCODING, CONTENT_LENGTH, CONTENT_RANGE, CONTENT_TYPE, ETAG, HeaderMap, HeaderValue, LAST_MODIFIED, LOCATION, RANGE, VARY};
use http::{Method, StatusCode};
use tokio::io::AsyncSeekExt;

use crate::core::error::{AppError, AppResult};
use crate::core::log::warn;
use crate::http::body::Body;
use crate::http::encode::Compression;
use crate::http::request::Request;
use crate::http::response::{Res, Response};
use super::arch::{Fetch, FileCache, Files, Key, Loaded, Range};

impl Files {

    pub fn new ( root: &Path, index: Option<&str>, cache_control: Option<&str>, autoindex: bool ) -> AppResult<Self> {

        let root = std::fs::canonicalize(root).map_err(|error| AppError::config("root", format!("cannot open `{}`: {error}", root.display())))?;

        if !root.is_dir() { return Err(AppError::config("root", format!("`{}` is not a directory", root.display()))); }

        let index = match index { Some("") => None, Some(name) => Some(name.to_string()), None => Some("index.html".to_string()) };

        if index.as_deref().is_some_and(|name| name.contains(['/', '\\'])) { return Err(AppError::config("index", "index must be a file name")); }

        let cache_control = cache_control.filter(|value| !value.is_empty()).map(HeaderValue::from_str).transpose().map_err(|_| AppError::config("cache_control", "invalid header value"))?;

        Ok(Self { root, index, cache_control, autoindex })

    }

    pub async fn serve ( &self, cache: &FileCache, fetch: Fetch<'_> ) -> Res {

        let Fetch { method, headers, path, strip, query, now_ms } = fetch;

        if method != Method::GET && method != Method::HEAD { return Self::reads_only(); }

        let Some(mut key) = self.locate(Request::strip(path, strip), path.ends_with('/')) else { return Response::status(404); };

        if let Some(loaded) = cache.fresh(&key, now_ms) { return self.memory(&loaded, method, headers); }

        let mut meta = match tokio::fs::metadata(key.path()).await { Ok(meta) => meta, Err(error) => { cache.forget(&key); return Self::failure(key.path(), &error); } };
        let mut directory = None;

        if meta.is_dir() {

            if !path.ends_with('/') { return Self::slash(path, query); }

            let Some(index) = &self.index else { return if self.autoindex { self.listing(key.path(), path, method).await } else { Response::status(403) }; };
            let file = key.path().join(index);

            directory = Some(key);
            key = Key::File(file);

            if let Some(loaded) = cache.fresh(&key, now_ms) { return self.memory(&loaded, method, headers); }

            meta = match tokio::fs::metadata(key.path()).await {
                Ok(meta) => meta,
                Err(error) if self.autoindex && error.kind() == ErrorKind::NotFound => { return self.listing(directory.as_ref().map_or(key.path(), Key::path), path, method).await; }
                Err(error) => { cache.forget(&key); return Self::failure(key.path(), &error); }
            };

            if meta.is_dir() { return Response::status(403); }

        }

        self.deliver(cache, key.path(), &meta, directory, fetch).await

    }

    pub(super) async fn deliver ( &self, cache: &FileCache, full: &Path, meta: &Metadata, directory: Option<Key>, fetch: Fetch<'_> ) -> Res {

        match cache.load(full, meta, fetch.now_ms).await {
            Ok(Some(loaded)) => { if let Some(directory) = directory { cache.alias(directory, &loaded); } self.memory(&loaded, fetch.method, fetch.headers) }
            Ok(None) => self.stream(full, meta, fetch.method, fetch.headers).await,
            Err(error) => Self::failure(full, &error),
        }

    }

    pub(super) fn memory ( &self, loaded: &Loaded, method: &Method, headers: &HeaderMap ) -> Res {

        let mut response = Response::status(200);

        if !loaded.packed.is_empty() {

            response.headers_mut().insert(VARY, HeaderValue::from_static("accept-encoding"));

            if !headers.contains_key(RANGE) && let Some(packed) = loaded.packed.iter().find(|packed| Compression::accepted(headers, packed.coding.as_bytes())) {

                self.describe(response.headers_mut(), loaded.kind, &packed.etag, loaded.modified);
                response.headers_mut().remove(ACCEPT_RANGES);
                response.headers_mut().insert(CONTENT_ENCODING, packed.coding.clone());

                if Self::fresh(headers, &packed.etag, loaded.modified) { *response.status_mut() = StatusCode::NOT_MODIFIED; return response; }

                response.headers_mut().insert(CONTENT_LENGTH, HeaderValue::from(packed.bytes.len()));

                if method != Method::HEAD && !packed.bytes.is_empty() { *response.body_mut() = Body::bytes(packed.bytes.clone()); }

                return response;

            }

        }

        self.describe(response.headers_mut(), loaded.kind, &loaded.etag, loaded.modified);

        if Self::fresh(headers, &loaded.etag, loaded.modified) {

            *response.status_mut() = StatusCode::NOT_MODIFIED;
            response.headers_mut().remove(CONTENT_LENGTH);

            return response;

        }

        let length = loaded.length;

        match Self::range(headers, length, &loaded.etag, loaded.modified) {
            Range::Full => {

                response.headers_mut().insert(CONTENT_LENGTH, HeaderValue::from(length));

                if method != Method::HEAD && length > 0 { *response.body_mut() = Body::bytes(loaded.bytes.clone()); }

            }
            Range::Part(start, end) => {

                *response.status_mut() = StatusCode::PARTIAL_CONTENT;
                Self::header(response.headers_mut(), CONTENT_RANGE, format!("bytes {start}-{end}/{length}"));
                response.headers_mut().insert(CONTENT_LENGTH, HeaderValue::from(end - start + 1));

                if method != Method::HEAD { *response.body_mut() = Body::bytes(loaded.bytes.slice(start as usize..=end as usize)); }

            }
            Range::Many(parts) => {

                let boundary = format!("aegisx-{}", loaded.etag.to_str().unwrap_or("0").trim_matches('"').replace('-', ""));
                let mut chunks = VecDeque::with_capacity(parts.len() * 2 + 1);
                let mut total = 0usize;

                for ( start, end ) in &parts {

                    let head = Bytes::from(format!("\r\n--{boundary}\r\nContent-Type: {}\r\nContent-Range: bytes {start}-{end}/{length}\r\n\r\n", loaded.kind));
                    let slice = loaded.bytes.slice(*start as usize..=*end as usize);

                    total += head.len() + slice.len();
                    chunks.push_back(head);
                    chunks.push_back(slice);

                }

                let tail = Bytes::from(format!("\r\n--{boundary}--\r\n"));

                total += tail.len();
                chunks.push_back(tail);

                *response.status_mut() = StatusCode::PARTIAL_CONTENT;
                Self::header(response.headers_mut(), CONTENT_TYPE, format!("multipart/byteranges; boundary={boundary}"));
                response.headers_mut().insert(CONTENT_LENGTH, HeaderValue::from(total));

                if method != Method::HEAD { *response.body_mut() = Body::chunks(chunks); }

            }
            Range::Unsatisfiable => {

                *response.status_mut() = StatusCode::RANGE_NOT_SATISFIABLE;
                Self::header(response.headers_mut(), CONTENT_RANGE, format!("bytes */{length}"));
                response.headers_mut().insert(CONTENT_LENGTH, HeaderValue::from_static("0"));

            }
        }

        response

    }

    async fn stream ( &self, full: &Path, meta: &Metadata, method: &Method, headers: &HeaderMap ) -> Res {

        let length = meta.len();
        let modified = FileCache::seconds(meta);
        let etag = Self::etag(modified, length);
        let mut response = Response::status(200);

        self.describe(response.headers_mut(), Self::kind(full), &etag, modified);

        if Self::fresh(headers, &etag, modified) {

            *response.status_mut() = StatusCode::NOT_MODIFIED;
            response.headers_mut().remove(CONTENT_LENGTH);

            return response;

        }

        let ( start, count ) = match Self::range(headers, length, &etag, modified) {
            Range::Full | Range::Many(_) => ( 0, length ),
            Range::Part(start, end) => {

                *response.status_mut() = StatusCode::PARTIAL_CONTENT;
                Self::header(response.headers_mut(), CONTENT_RANGE, format!("bytes {start}-{end}/{length}"));

                ( start, end - start + 1 )

            }
            Range::Unsatisfiable => {

                *response.status_mut() = StatusCode::RANGE_NOT_SATISFIABLE;
                Self::header(response.headers_mut(), CONTENT_RANGE, format!("bytes */{length}"));

                ( 0, 0 )

            }
        };

        response.headers_mut().insert(CONTENT_LENGTH, HeaderValue::from(count));

        if method == Method::HEAD || count == 0 { return response; }

        let mut file = match tokio::fs::File::open(full).await { Ok(file) => file, Err(error) => return Self::failure(full, &error) };

        if start > 0 && let Err(error) = file.seek(SeekFrom::Start(start)).await { return Self::failure(full, &error); }

        *response.body_mut() = Body::file(file, count);

        response

    }

    pub(super) fn reads_only () -> Res {

        let mut response = Response::status(405);

        response.headers_mut().insert(ALLOW, HeaderValue::from_static("GET, HEAD"));

        response

    }

    pub(super) fn etag ( modified: Option<u64>, length: u64 ) -> HeaderValue {

        HeaderValue::from_str(&format!("\"{:x}-{length:x}\"", modified.unwrap_or(0))).unwrap_or_else(|_| HeaderValue::from_static("\"0\""))

    }

    pub(super) fn locate ( &self, relative: &str, directory: bool ) -> Option<Key> {

        let decoded = Request::decode(relative.trim_start_matches('/'));
        let tail = Self::os_path(&decoded)?;

        if tail.components().any(|component| !matches!(component, Component::Normal(_))) { return None; }

        let full = self.root.join(tail);

        Some(if directory { Key::Dir(full) } else { Key::File(full) })

    }

    #[cfg(unix)]
    fn os_path ( bytes: &[u8] ) -> Option<PathBuf> {

        use std::os::unix::ffi::OsStrExt;

        if bytes.contains(&0) { return None; }

        Some(PathBuf::from(std::ffi::OsStr::from_bytes(bytes)))

    }

    #[cfg(not(unix))]
    fn os_path ( bytes: &[u8] ) -> Option<PathBuf> {

        let text = std::str::from_utf8(bytes).ok()?;

        if text.contains(['\\', ':', '\0']) { return None; }

        Some(PathBuf::from(text))

    }

    fn describe ( &self, headers: &mut HeaderMap, kind: &'static str, etag: &HeaderValue, modified: Option<u64> ) {

        headers.insert(CONTENT_TYPE, HeaderValue::from_static(kind));
        headers.insert(ACCEPT_RANGES, HeaderValue::from_static("bytes"));
        headers.insert(ETAG, etag.clone());

        if let Some(seconds) = modified { Self::header(headers, LAST_MODIFIED, httpdate::fmt_http_date(UNIX_EPOCH + std::time::Duration::from_secs(seconds))); }

        if let Some(value) = &self.cache_control { headers.insert(CACHE_CONTROL, value.clone()); }

    }

    fn slash ( path: &str, query: Option<&str> ) -> Res {

        let mut response = Response::status(301);
        let location = match query { Some(query) => format!("{path}/?{query}"), None => format!("{path}/") };

        Self::header(response.headers_mut(), LOCATION, location);

        response

    }

    pub(super) fn failure ( full: &Path, error: &std::io::Error ) -> Res {

        match error.kind() {
            ErrorKind::NotFound | ErrorKind::NotADirectory | ErrorKind::InvalidInput | ErrorKind::InvalidFilename => Response::status(404),
            ErrorKind::PermissionDenied => Response::status(403),
            _ => { warn!(%error, path = %full.display(), "static file failed"); Response::status(500) }
        }

    }

    pub(super) fn header ( headers: &mut HeaderMap, name: http::header::HeaderName, value: String ) {

        if let Ok(value) = HeaderValue::from_str(&value) { headers.insert(name, value); }

    }

}
