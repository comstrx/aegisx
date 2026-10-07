use http::header::{ACCEPT_ENCODING, ACCEPT_RANGES, CACHE_CONTROL, CONTENT_ENCODING, CONTENT_LENGTH, CONTENT_RANGE, CONTENT_TYPE, ETAG, HeaderMap, HeaderValue, VARY};
use http_body::Body as _;

use crate::http::body::Body;
use crate::http::response::Res;
use super::arch::{Codec, Compression, Encoded, Encoding};

impl Compression {

    pub fn new ( min_bytes: u64, level: u32, brotli: bool, brotli_level: u32, zstd: bool, zstd_level: u32, types: impl IntoIterator<Item = String> ) -> Self {

        let types = types.into_iter().map(|kind| kind.trim().to_ascii_lowercase().into_boxed_str()).filter(|kind| !kind.is_empty()).collect();

        Self { min_bytes, level: level.clamp(1, 9), brotli, brotli_level: brotli_level.min(11), zstd, zstd_level: zstd_level.clamp(1, 19) as i32, types }

    }

    pub fn accepts_gzip ( headers: &HeaderMap ) -> bool {

        let ( gzip, _, _, any ) = Self::offered(headers);

        gzip.unwrap_or_else(|| any.unwrap_or(false))

    }

    pub fn accepted ( headers: &HeaderMap, coding: &[u8] ) -> bool {

        let ( gzip, brotli, zstd, any ) = Self::offered(headers);

        match coding {
            b"br" => brotli,
            b"zstd" => zstd,
            b"gzip" => gzip,
            _ => Some(false),
        }.unwrap_or_else(|| any.unwrap_or(false))

    }

    fn offered ( headers: &HeaderMap ) -> ( Option<bool>, Option<bool>, Option<bool>, Option<bool> ) {

        let ( mut gzip, mut brotli, mut zstd, mut any ) = ( None, None, None, None );

        for value in headers.get_all(ACCEPT_ENCODING) {

            let Ok(text) = value.to_str() else { return ( Some(false), Some(false), Some(false), Some(false) ); };

            for part in text.split(',') {

                let mut pieces = part.split(';');
                let name = pieces.next().unwrap_or("").trim();
                let wanted = pieces.find_map(|piece| piece.trim().strip_prefix("q=").map(|q| q.trim().parse::<f32>().is_ok_and(|q| q > 0.0))).unwrap_or(true);

                if name.eq_ignore_ascii_case("zstd") { zstd = Some(wanted); }
                else if name.eq_ignore_ascii_case("br") { brotli = Some(wanted); }
                else if name.eq_ignore_ascii_case("gzip") || name.eq_ignore_ascii_case("x-gzip") { gzip = Some(wanted); }
                else if name == "*" { any = Some(wanted); }

            }

        }

        ( gzip, brotli, zstd, any )

    }

    pub fn pick ( &self, headers: &HeaderMap ) -> Option<Encoding> {

        let ( gzip, brotli, zstd, any ) = Self::offered(headers);
        let accepts = |explicit: Option<bool>| explicit.unwrap_or_else(|| any.unwrap_or(false));

        if self.zstd && accepts(zstd) { return Some(Encoding::Zstd); }

        if self.brotli && accepts(brotli) { return Some(Encoding::Brotli); }

        accepts(gzip).then_some(Encoding::Gzip)

    }

    pub fn apply ( &self, encoding: Encoding, response: &mut Res ) -> bool {

        if !matches!(response.status().as_u16(), 200 | 403 | 404) || response.body().is_end_stream() { return false; }

        let headers = response.headers();

        if headers.contains_key(CONTENT_ENCODING) || headers.contains_key(CONTENT_RANGE) { return false; }

        if !self.typed(headers.get(CONTENT_TYPE)) { return false; }

        if headers.get(CONTENT_LENGTH).and_then(|value| value.to_str().ok()).and_then(|value| value.parse::<u64>().ok()).is_some_and(|length| length < self.min_bytes) { return false; }

        if headers.get_all(CACHE_CONTROL).iter().any(|value| Self::lists(value.as_bytes(), b"no-transform")) { return false; }

        let Ok(codec) = Codec::new(encoding, self.level, self.brotli_level, self.zstd_level) else { return false; };
        let headers = response.headers_mut();

        headers.remove(CONTENT_LENGTH);
        headers.remove(ACCEPT_RANGES);
        headers.insert(CONTENT_ENCODING, HeaderValue::from_static(match encoding { Encoding::Gzip => "gzip", Encoding::Brotli => "br", Encoding::Zstd => "zstd" }));

        match headers.get(VARY).map(|value| value.as_bytes()) {
            None => { headers.insert(VARY, HeaderValue::from_static("Accept-Encoding")); }
            Some(b"*") => {}
            Some(existing) if Self::lists(existing, b"accept-encoding") => {}
            Some(existing) => {

                let mut merged = existing.to_vec();

                merged.extend_from_slice(b", Accept-Encoding");

                if let Ok(value) = HeaderValue::from_bytes(&merged) { headers.insert(VARY, value); }

            }
        }

        if let Some(etag) = headers.get(ETAG) && etag.as_bytes().first() == Some(&b'"') {

            let mut weak = Vec::with_capacity(etag.len() + 2);

            weak.extend_from_slice(b"W/");
            weak.extend_from_slice(etag.as_bytes());

            if let Ok(value) = HeaderValue::from_bytes(&weak) { headers.insert(ETAG, value); }

        }

        let inner = std::mem::replace(response.body_mut(), Body::Empty);

        *response.body_mut() = Body::encoded(Encoded::new(inner, codec));

        true

    }

    fn typed ( &self, value: Option<&HeaderValue> ) -> bool {

        let Some(kind) = value.and_then(|value| value.to_str().ok()) else { return false; };
        let media = kind.split(';').next().unwrap_or("").trim();

        self.types.iter().any(|allowed| allowed.eq_ignore_ascii_case(media))

    }

    fn lists ( header: &[u8], token: &[u8] ) -> bool {

        header.split(|byte| *byte == b',').any(|candidate| candidate.trim_ascii().eq_ignore_ascii_case(token))

    }

}
