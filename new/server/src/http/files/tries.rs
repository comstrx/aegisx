use http::Method;

use crate::http::request::Request;
use crate::http::response::{Res, Response};
use super::arch::{Candidate, Fetch, FileCache, Files, Key};

impl Candidate {

    pub fn parse ( text: &str ) -> Self {

        if let Some(code) = text.strip_prefix('=') { return Self::Status(code.parse().unwrap_or(404)); }

        match text {
            "$uri" => Self::Uri,
            "$uri/" => Self::Dir,
            "@upstream" => Self::Upstream,
            path => Self::Path(path.into()),
        }

    }

}

impl Files {

    pub async fn attempt ( &self, cache: &FileCache, fetch: Fetch<'_>, candidates: &[Candidate] ) -> Option<Res> {

        let Fetch { method, headers, path, strip, now_ms, .. } = fetch;

        if method != Method::GET && method != Method::HEAD { return (candidates.last() != Some(&Candidate::Upstream)).then(Self::reads_only); }

        let relative = Request::strip(path, strip);

        for candidate in candidates {

            let ( target, directory ) = match candidate {
                Candidate::Status(code) => return Some(Response::status(*code)),
                Candidate::Upstream => return None,
                Candidate::Uri => ( relative, false ),
                Candidate::Dir => ( relative, true ),
                Candidate::Path(path) => ( path.as_ref(), path.ends_with('/') ),
            };

            if !directory && target.ends_with('/') { continue; }

            let Some(key) = self.locate(target, directory) else { continue; };

            if let Some(loaded) = cache.fresh(&key, now_ms) { return Some(self.memory(&loaded, method, headers)); }

            let Ok(meta) = tokio::fs::metadata(key.path()).await else { cache.forget(&key); continue; };

            if meta.is_dir() != directory { continue; }

            if !directory { return Some(self.deliver(cache, key.path(), &meta, None, fetch).await); }

            let Some(index) = &self.index else { return Some(if self.autoindex { self.listing(key.path(), path, method).await } else { Response::status(403) }); };
            let file = Key::File(key.path().join(index));

            if let Some(loaded) = cache.fresh(&file, now_ms) { return Some(self.memory(&loaded, method, headers)); }

            return Some(match tokio::fs::metadata(file.path()).await {
                Ok(meta) if meta.is_dir() => Response::status(403),
                Ok(meta) => self.deliver(cache, file.path(), &meta, Some(key), fetch).await,
                Err(error) => Self::failure(file.path(), &error),
            });

        }

        Some(Response::status(404))

    }

}
