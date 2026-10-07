use http::header::{HeaderMap, HeaderName, HeaderValue, IF_MODIFIED_SINCE, IF_NONE_MATCH};

use crate::core::str::Str;
use super::arch::{HOP, Header, Rendered};

impl Header {

    pub fn is_hop ( name: &HeaderName ) -> bool {

        HOP.iter().any(|hop| hop == name)

    }

    pub fn token ( token: &[u8] ) -> Option<HeaderName> {

        let token = token.trim_ascii();

        if token.eq_ignore_ascii_case(b"close") || HOP.iter().any(|hop| token.eq_ignore_ascii_case(hop.as_str().as_bytes())) { return None; }

        HeaderName::from_bytes(token).ok()

    }

    pub fn apply ( headers: &mut HeaderMap, rules: &[( HeaderName, Rendered )] ) {

        for ( name, rule ) in rules {

            match rule {
                Rendered::Static(value) => { headers.insert(name.clone(), value.clone()); }
                Rendered::Append(value) => { headers.append(name.clone(), value.clone()); }
                Rendered::Default(value) => { if !headers.contains_key(name) { headers.insert(name.clone(), value.clone()); } }
                Rendered::Remove => { headers.remove(name); }
                Rendered::Dynamic(_) => {}
            }

        }

    }

    pub fn authority ( text: &str ) -> Option<HeaderValue> {

        Self::static_value(text)

    }

    pub fn static_name ( text: &str ) -> Option<HeaderName> {

        HeaderName::from_bytes(text.as_bytes()).ok()?;

        Some(HeaderName::from_static(Str::intern(&text.to_ascii_lowercase())))

    }

    pub fn fresh ( headers: &HeaderMap, etag: Option<&HeaderValue>, modified: Option<u64> ) -> bool {

        if headers.contains_key(IF_NONE_MATCH) {

            let Some(etag) = etag else { return false; };

            return headers.get_all(IF_NONE_MATCH).iter().any(|value| Self::listed(value.as_bytes(), etag.as_bytes()));

        }

        match ( headers.get(IF_MODIFIED_SINCE).and_then(Self::date), modified ) {
            ( Some(since), Some(modified) ) => modified <= since,
            _ => false,
        }

    }

    pub fn listed ( header: &[u8], etag: &[u8] ) -> bool {

        header.split(|byte| *byte == b',').map(<[u8]>::trim_ascii).any(|candidate| candidate == b"*" || candidate.strip_prefix(b"W/").unwrap_or(candidate) == etag.strip_prefix(b"W/").unwrap_or(etag))

    }

    pub fn date ( value: &HeaderValue ) -> Option<u64> {

        value.to_str().ok().and_then(|text| httpdate::parse_http_date(text).ok()).and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok()).map(|since| since.as_secs())

    }

    pub fn static_value ( text: &str ) -> Option<HeaderValue> {

        HeaderValue::from_str(text).ok()?;

        Some(HeaderValue::from_static(Str::intern(text)))

    }

}
