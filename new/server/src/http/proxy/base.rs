use std::borrow::Cow;
use std::future::Future;
use std::time::Instant;

use http::Uri;
use http::header::{CONNECTION, CONTENT_LENGTH, COOKIE, HOST, HeaderMap, HeaderName, HeaderValue, TE, TRAILER, TRANSFER_ENCODING, UPGRADE};
use http_body::Body as _;

use crate::core::list::Few;
use crate::http::body::Body;
use crate::http::header::Header;
use crate::http::request::{Req, Request, Shadow};
use crate::http::response::Res;
use crate::http::upstream::{Client, Failure, Replay, Timing, Upstream};
use super::arch::{Forward, Plan, Proxy, Target};

impl Proxy {

    pub fn forward <'f> ( client: &'f Client, forward: &'f Forward<'f>, started: Instant, replay: Replay, request: Box<Req<Body>> ) -> impl Future<Output = Result<( Res<Body>, Option<Shadow> ), Failure>> + 'f {

        client.exchange(forward.upstream, Timing { started, timeout_ms: forward.plan.timeout_ms }, replay, request, move |request| Self::prepare(request, forward))

    }

    pub fn settle ( response: &mut Res<Body>, plan: &Plan, upstream: &Upstream ) {

        Self::sanitize(response);

        if plan.edits() { Self::edit(response.headers_mut(), plan, upstream); }

        Header::apply(response.headers_mut(), &plan.response_headers);

    }

    fn locate ( uri: &Uri, strip: usize, target: Target<'_> ) -> Option<Uri> {

        if target == Target::Request && strip == 0 && uri.scheme().is_none() && uri.authority().is_none() { return None; }

        Self::target(uri, strip, target).parse().ok()

    }

    fn host ( request: &Req<Body>, forward: &Forward<'_> ) -> HeaderValue {

        if !forward.plan.preserve_host { return forward.upstream.authority.clone(); }

        if let Some(host) = request.headers().get(HOST) { return host.clone(); }

        request.uri().authority().and_then(|authority| HeaderValue::from_str(authority.as_str()).ok()).unwrap_or_else(|| forward.upstream.authority.clone())

    }

    fn cookies ( headers: &HeaderMap ) -> Option<HeaderValue> {

        let mut values = headers.get_all(COOKIE).iter();
        let first = values.next()?;
        let second = values.next()?;
        let mut joined = Vec::with_capacity(first.len() + second.len() + 2);

        joined.extend_from_slice(first.as_bytes());

        for value in std::iter::once(second).chain(values) {

            joined.extend_from_slice(b"; ");
            joined.extend_from_slice(value.as_bytes());

        }

        HeaderValue::from_bytes(&joined).ok()

    }

    fn prepare ( request: &mut Req<Body>, forward: &Forward<'_> ) -> Option<Uri> {

        let exact = request.body().size_hint().exact();
        let length = match exact { Some(length) if length > 0 || request.headers().contains_key(CONTENT_LENGTH) => Some(length), _ => None };
        let host = Self::host(request, forward);
        let cookie = (request.version() >= http::Version::HTTP_2).then(|| Self::cookies(request.headers())).flatten();
        let origin = Self::locate(request.uri(), forward.plan.mount.len(), forward.target).map(|uri| std::mem::replace(request.uri_mut(), uri));

        *request.version_mut() = http::Version::HTTP_11;

        let headers = request.headers_mut();

        Self::strip(headers, forward.drop, exact.is_none(), forward.plan.underscores);

        headers.reserve(forward.plan.request_headers.len() + forward.rendered.len() + forward.add.len() + 3);
        headers.insert(HOST, host);

        if let Some(cookie) = cookie { headers.insert(COOKIE, cookie); }

        Header::apply(headers, &forward.plan.request_headers);

        for ( name, value ) in forward.rendered { headers.insert(name.clone(), value.clone()); }

        for ( name, value ) in forward.add.iter().flatten() { headers.insert(*name, (*value).clone()); }

        if let Some(upgrade) = forward.upgrade {

            headers.insert(CONNECTION, HeaderValue::from_static("upgrade"));
            headers.insert(UPGRADE, upgrade.clone());

        }

        match ( length, exact ) {
            ( Some(length), _ ) => { headers.insert(CONTENT_LENGTH, HeaderValue::from(length)); }
            ( None, None ) => { headers.remove(CONTENT_LENGTH); headers.insert(TRANSFER_ENCODING, HeaderValue::from_static("chunked")); }
            ( None, Some(_) ) => {}
        }

        origin

    }

    fn strip ( headers: &mut HeaderMap, drop: &[HeaderName], streamed: bool, underscores: bool ) {

        let mut doomed: Few<HeaderName, 8> = Few::new();
        let mut spill: Vec<HeaderName> = Vec::new();
        let mut trailers = false;

        for token in headers.get_all(CONNECTION).iter().flat_map(|value| value.as_bytes().split(|byte| *byte == b',')) {

            if let Some(name) = Header::token(token) && !doomed.push(name.clone()) { spill.push(name); }

        }

        for ( name, value ) in headers.iter() {

            if *name == TE { trailers = trailers || value.as_bytes().split(|byte| *byte == b',').any(|token| token.trim_ascii().eq_ignore_ascii_case(b"trailers")); }

            let unwanted = (Header::is_hop(name) && !(streamed && *name == TRAILER)) || drop.contains(name) || (!underscores && name.as_str().as_bytes().contains(&b'_'));

            if unwanted && !doomed.iter().any(|seen| seen == name) && !doomed.push(name.clone()) { spill.push(name.clone()); }

        }

        for name in doomed.iter().chain(spill.iter()) { headers.remove(name); }

        if trailers { headers.insert(TE, HeaderValue::from_static("trailers")); }

    }

    fn sanitize ( response: &mut Res<Body> ) {

        let upgrading = response.status() == http::StatusCode::SWITCHING_PROTOCOLS;
        let mut listed: Few<HeaderName, 8> = Few::new();

        for token in response.headers().get_all(CONNECTION).iter().flat_map(|value| value.as_bytes().split(|byte| *byte == b',')) {

            if let Some(name) = Header::token(token) { listed.push(name); }

        }

        let headers = response.headers_mut();
        let mut present: Few<HeaderName, 8> = Few::new();

        for ( name, _ ) in headers.iter() {

            let hop = (Header::is_hop(name) && *name != TRAILER && !(upgrading && *name == UPGRADE)) || *name == TRANSFER_ENCODING || listed.iter().any(|hop| hop == name);

            if hop && !present.iter().any(|seen| seen == name) && !present.push(name.clone()) { break; }

        }

        for name in present.iter() { headers.remove(name); }

    }

    pub fn target <'u> ( uri: &'u Uri, strip: usize, target: Target<'u> ) -> Cow<'u, str> {

        let path = match target {
            Target::Rewritten(target) => return Cow::Borrowed(target),
            Target::Request if strip == 0 => return Cow::Borrowed(Request::target(uri)),
            Target::Request => Request::strip(uri.path(), strip),
            Target::Canonical(path) => Request::strip(path, strip),
        };

        match uri.query() {
            Some(query) => Cow::Owned(format!("{path}?{query}")),
            None => Cow::Borrowed(path),
        }

    }

}
