use http::header::{HeaderMap, HeaderValue, IF_RANGE, RANGE};
use http_range_header::{EndPosition, ParsedRanges, RangeUnsatisfiableError, StartPosition};

use crate::http::header::Header;

use super::arch::{Files, Range};

impl Files {

    pub(super) fn fresh ( headers: &HeaderMap, etag: &HeaderValue, modified: Option<u64> ) -> bool {

        Header::fresh(headers, Some(etag), modified)

    }

    pub(super) fn range ( headers: &HeaderMap, length: u64, etag: &HeaderValue, modified: Option<u64> ) -> Range {

        let Some(spec) = headers.get(RANGE).and_then(|value| value.to_str().ok()) else { return Range::Full; };

        if let Some(condition) = headers.get(IF_RANGE) {

            let matches = match condition.as_bytes().first() {
                Some(b'"') | Some(b'W') => condition.as_bytes() == etag.as_bytes(),
                _ => Header::date(condition).is_some_and(|date| modified == Some(date)),
            };

            if !matches { return Range::Full; }

        }

        if length == 0 { return Range::Full; }

        let parsed = match http_range_header::parse_range_header(spec.trim()) {
            Ok(parsed) if parsed.ranges.len() <= 16 => parsed,
            Err(RangeUnsatisfiableError::ZeroSuffix) => return Range::Unsatisfiable,
            _ => return Range::Full,
        };

        if parsed.ranges.iter().any(|range| matches!(( range.start, range.end ), ( StartPosition::Index(start), EndPosition::Index(end) ) if start > end)) { return Range::Full; }

        let mut parts = Vec::with_capacity(parsed.ranges.len());

        for range in parsed.ranges {

            match (ParsedRanges { ranges: vec![range] }).validate(length) {
                Ok(valid) => parts.extend(valid.into_iter().map(|part| ( *part.start(), *part.end() ))),
                Err(RangeUnsatisfiableError::FileSuffixOutOfBounds) => parts.push(( 0, length - 1 )),
                Err(_) => {}
            }

        }

        match parts.len() {
            0 => Range::Unsatisfiable,
            1 => Range::Part(parts[0].0, parts[0].1),
            _ => Range::Many(parts),
        }

    }

}
