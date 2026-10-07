use http::header::{Entry, HeaderMap, HeaderName, HeaderValue, LOCATION, REFRESH, SET_COOKIE};

use crate::http::upstream::Upstream;
use super::arch::{Edit, Plan, Proxy};

impl Edit {

    pub fn new ( from: &str, to: &str ) -> Self {

        Self { from: from.into(), to: to.into() }

    }

}

impl Proxy {

    pub(super) fn edit ( headers: &mut HeaderMap, plan: &Plan, upstream: &Upstream ) {

        if let Some(edits) = &plan.redirects {

            for ( name, refresh ) in [( LOCATION, false ), ( REFRESH, true )] {

                Self::each_value(headers, name, |value| Self::relocate(value, refresh, edits, plan, upstream));

            }

        }

        if !plan.cookie_domain.is_empty() || !plan.cookie_path.is_empty() {

            Self::each_value(headers, SET_COOKIE, |value| Self::cookie(value, &plan.cookie_domain, &plan.cookie_path));

        }

    }

    fn each_value ( headers: &mut HeaderMap, name: HeaderName, mut edit: impl FnMut(&[u8]) -> Option<HeaderValue> ) {

        if let Entry::Occupied(mut entry) = headers.entry(name) {

            for value in entry.iter_mut() { if let Some(edited) = edit(value.as_bytes()) { *value = edited; } }

        }

    }

    fn relocate ( value: &[u8], refresh: bool, edits: &[Edit], plan: &Plan, upstream: &Upstream ) -> Option<HeaderValue> {

        let start = if refresh { Self::url_start(value)? } else { 0 };
        let ( lead, url ) = value.split_at(start);
        let ( head, rest ) = Self::replacement(url, edits, plan, upstream)?;

        let mut edited = Vec::with_capacity(lead.len() + head.len() + rest.len());

        edited.extend_from_slice(lead);
        edited.extend_from_slice(head);
        edited.extend_from_slice(rest);

        HeaderValue::from_bytes(&edited).ok()

    }

    fn url_start ( value: &[u8] ) -> Option<usize> {

        let semicolon = value.iter().position(|byte| *byte == b';')?;
        let tail = &value[semicolon + 1..];
        let offset = tail.windows(4).position(|window| window.eq_ignore_ascii_case(b"url="))?;

        Some(semicolon + 1 + offset + 4)

    }

    fn replacement <'v> ( url: &'v [u8], edits: &'v [Edit], plan: &'v Plan, upstream: &Upstream ) -> Option<( &'v [u8], &'v [u8] )> {

        if edits.is_empty() {

            let rest = Self::own(url, upstream.authority.as_bytes())?;

            return Some(( Self::mount(plan, rest), rest ));

        }

        for edit in edits {

            let Some(rest) = url.strip_prefix(edit.from.as_bytes()) else { continue; };

            return Some(if edit.to.is_empty() { ( Self::mount(plan, rest), rest ) } else { ( edit.to.as_bytes(), rest ) });

        }

        None

    }

    fn own <'v> ( url: &'v [u8], authority: &[u8] ) -> Option<&'v [u8]> {

        let scheme = if url.len() >= 8 && url[..8].eq_ignore_ascii_case(b"https://") { 8 } else if url.len() >= 7 && url[..7].eq_ignore_ascii_case(b"http://") { 7 } else { return None; };
        let rest = &url[scheme..];

        if rest.len() < authority.len() || !rest[..authority.len()].eq_ignore_ascii_case(authority) { return None; }

        let rest = &rest[authority.len()..];

        matches!(rest.first(), None | Some(b'/' | b'?' | b'#')).then_some(rest)

    }

    fn mount <'p> ( plan: &'p Plan, rest: &[u8] ) -> &'p [u8] {

        if plan.mount.is_empty() && rest.first() != Some(&b'/') { b"/" } else { plan.mount.as_bytes() }

    }

    fn cookie ( value: &[u8], domains: &[Edit], paths: &[Edit] ) -> Option<HeaderValue> {

        let mut edited: Option<Vec<u8>> = None;
        let mut copied = 0;
        let mut cursor = value.iter().position(|byte| *byte == b';').map_or(value.len(), |end| end + 1);

        while cursor < value.len() {

            let end = value[cursor..].iter().position(|byte| *byte == b';').map_or(value.len(), |end| cursor + end);
            let segment = &value[cursor..end];

            cursor = end + 1;

            let Some(equals) = segment.iter().position(|byte| *byte == b'=') else { continue; };
            let name = segment[..equals].trim_ascii();
            let raw = &segment[equals + 1..];
            let current = raw.trim_ascii();
            let start = end - raw.len() + (raw.len() - raw.trim_ascii_start().len());

            let replacement = if name.eq_ignore_ascii_case(b"domain") { Self::domain(current, domains) } else if name.eq_ignore_ascii_case(b"path") { Self::path(current, paths) } else { None };

            let Some(( head, tail )) = replacement else { continue; };
            let out = edited.get_or_insert_with(|| Vec::with_capacity(value.len() + 32));

            out.extend_from_slice(&value[copied..start]);
            out.extend_from_slice(head);
            out.extend_from_slice(tail);

            copied = start + current.len();

        }

        let mut out = edited?;

        out.extend_from_slice(&value[copied..]);

        HeaderValue::from_bytes(&out).ok()

    }

    fn domain <'e> ( current: &[u8], edits: &'e [Edit] ) -> Option<( &'e [u8], &'e [u8] )> {

        let bare = current.strip_prefix(b".").unwrap_or(current);

        edits.iter().find(|edit| edit.from.trim_start_matches('.').as_bytes().eq_ignore_ascii_case(bare)).map(|edit| ( edit.to.as_bytes(), &b""[..] ))

    }

    fn path <'v> ( current: &'v [u8], edits: &'v [Edit] ) -> Option<( &'v [u8], &'v [u8] )> {

        edits.iter().find_map(|edit| current.strip_prefix(edit.from.as_bytes()).map(|rest| ( edit.to.as_bytes(), rest )))

    }

}
