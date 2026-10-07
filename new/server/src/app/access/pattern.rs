use std::io::Write;

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use http::header::{AUTHORIZATION, HOST, HeaderMap, HeaderName};

use crate::core::error::{AppError, AppResult};
use crate::core::time::Clock;
use super::arch::{Capture, Entry, Field, Log, Pattern, Piece};

const CAPTURES_MAX: usize = 8;

impl Pattern {

    pub fn compile ( text: &str ) -> AppResult<Self> {

        let mut pieces = Vec::new();
        let mut captures: Vec<Capture> = Vec::new();
        let mut literal = Vec::new();
        let bytes = text.as_bytes();
        let mut index = 0;

        while index < bytes.len() {

            if bytes[index] != b'$' { literal.push(bytes[index]); index += 1; continue; }

            let ( name, consumed ) = match bytes.get(index + 1) {
                Some(b'{') => {

                    let close = bytes[index..].iter().position(|byte| *byte == b'}').ok_or_else(|| AppError::config("set_access_log", format!("unterminated variable in pattern `{text}`")))?;

                    ( &text[index + 2..index + close], close + 1 )

                }
                Some(byte) if byte.is_ascii_alphabetic() || *byte == b'_' => {

                    let end = bytes[index + 1..].iter().position(|byte| !(byte.is_ascii_alphanumeric() || *byte == b'_')).map_or(bytes.len(), |offset| index + 1 + offset);

                    ( &text[index + 1..end], end - index )

                }
                _ => { literal.push(b'$'); index += 1; continue; }
            };

            let field = Self::field(name, &mut captures).ok_or_else(|| AppError::config("set_access_log", format!("unknown variable `${name}` in pattern")))?;

            if captures.len() > CAPTURES_MAX { return Err(AppError::config("set_access_log", format!("pattern captures more than {CAPTURES_MAX} request headers"))); }

            if !literal.is_empty() { pieces.push(Piece::Text(std::mem::take(&mut literal).into_boxed_slice())); }

            pieces.push(Piece::Field(field));
            index += consumed;

        }

        if !literal.is_empty() { pieces.push(Piece::Text(literal.into_boxed_slice())); }

        pieces.push(Piece::Text(Box::new(*b"\n")));

        Ok(Self { pieces, captures })

    }

    fn field ( name: &str, captures: &mut Vec<Capture> ) -> Option<Field> {

        let mut capture = |capture: Capture| {

            let index = captures.iter().position(|existing| *existing == capture).unwrap_or_else(|| { captures.push(capture); captures.len() - 1 });

            Field::Capture(index)

        };

        Some(match name {
            "remote_addr" => Field::RemoteAddr,
            "remote_port" => Field::RemotePort,
            "remote_user" => capture(Capture::User),
            "time_local" => Field::TimeLocal,
            "time_iso8601" => Field::TimeIso8601,
            "msec" => Field::Msec,
            "request" => Field::Request,
            "request_method" => Field::RequestMethod,
            "request_uri" => Field::RequestUri,
            "uri" | "document_uri" => Field::Uri,
            "args" | "query_string" => Field::Args,
            "server_protocol" => Field::ServerProtocol,
            "status" => Field::Status,
            "body_bytes_sent" | "bytes_sent" => Field::BodyBytesSent,
            "request_length" | "bytes_received" => Field::RequestLength,
            "http_referer" => Field::HttpReferer,
            "http_user_agent" => Field::HttpUserAgent,
            "host" | "http_host" => capture(Capture::Header(HOST)),
            "request_time" => Field::RequestTime,
            "request_time_us" => Field::RequestTimeUs,
            "upstream_addr" => Field::UpstreamAddr,
            "upstream_response_time" | "upstream_header_time" => Field::UpstreamResponseTime,
            "upstream_status" => Field::UpstreamStatus,
            "upstream_attempts" => Field::UpstreamAttempts,
            "route" => Field::Route,
            "request_id" => Field::RequestId,
            "scheme" => Field::Scheme,
            "ssl_client_s_dn" => capture(Capture::ClientDn),
            other => {

                let header = other.strip_prefix("http_")?;
                let name = HeaderName::from_bytes(header.replace('_', "-").as_bytes()).ok()?;

                capture(Capture::Header(name))

            }
        })

    }

    pub(super) fn capture ( &self, headers: &HeaderMap, client: Option<&str>, entry: &mut Entry ) {

        for capture in &self.captures {

            match capture {
                Capture::Header(name) => { if let Some(value) = headers.get(name) { entry.captured.extend_from_slice(value.as_bytes()); } }
                Capture::ClientDn => { if let Some(dn) = client { entry.captured.extend_from_slice(dn.as_bytes()); } }
                Capture::User => {

                    if let Some(value) = headers.get(AUTHORIZATION) && value.len() > 6 && value.as_bytes()[..6].eq_ignore_ascii_case(b"basic ") && let Ok(decoded) = STANDARD.decode(value.as_bytes()[6..].trim_ascii()) {

                        let user = decoded.split(|byte| *byte == b':').next().unwrap_or(&[]);

                        entry.captured.extend_from_slice(user);

                    }

                }
            }

            entry.cuts.push(entry.captured.len().min(u16::MAX as usize) as u16);

        }

    }

}

impl Log {

    pub(super) fn custom ( out: &mut Vec<u8>, stamp: &[u8; 26], entry: &Entry, pattern: &Pattern ) {

        for piece in &pattern.pieces {

            let field = match piece { Piece::Text(text) => { out.extend_from_slice(text); continue; } Piece::Field(field) => *field };

            match field {
                Field::RemoteAddr => Self::address(out, entry.peer.ip()),
                Field::RemotePort => Self::number(out, u64::from(entry.peer.port())),
                Field::TimeLocal => out.extend_from_slice(stamp),
                Field::TimeIso8601 => Self::iso8601(out, entry.started_ms),
                Field::Msec => { Self::number(out, entry.started_ms / 1_000); out.push(b'.'); Self::padded(out, entry.started_ms % 1_000, 3); }
                Field::Request => {

                    out.extend_from_slice(entry.method.as_str().as_bytes());
                    out.push(b' ');
                    Self::quoted(out, entry.target());
                    out.push(b' ');
                    out.extend_from_slice(Self::version(entry.version));

                }
                Field::RequestMethod => out.extend_from_slice(entry.method.as_str().as_bytes()),
                Field::RequestUri => Self::quoted(out, entry.target()),
                Field::Uri => Self::quoted(out, entry.target().split(|byte| *byte == b'?').next().unwrap_or(b"/")),
                Field::Args => Self::quoted(out, entry.target().splitn(2, |byte| *byte == b'?').nth(1).unwrap_or(b"")),
                Field::ServerProtocol => out.extend_from_slice(Self::version(entry.version)),
                Field::Status => Self::number(out, u64::from(entry.status)),
                Field::BodyBytesSent => Self::number(out, entry.sent() as u64),
                Field::RequestLength => Self::number(out, entry.received),
                Field::HttpReferer => Self::quoted(out, Self::dash(entry.referer())),
                Field::HttpUserAgent => Self::quoted(out, Self::dash(entry.agent())),
                Field::RequestTime => { let elapsed = Clock::elapsed_us(entry.started); Self::number(out, elapsed / 1_000_000); out.push(b'.'); Self::padded(out, (elapsed % 1_000_000) / 1_000, 3); }
                Field::RequestTimeUs => Self::number(out, Clock::elapsed_us(entry.started)),
                Field::UpstreamAddr => match &entry.backend { Some(addr) => Self::address_of(out, addr), None => out.push(b'-') },
                Field::UpstreamResponseTime => match entry.backend { Some(_) => { Self::number(out, entry.header_us / 1_000_000); out.push(b'.'); Self::padded(out, (entry.header_us % 1_000_000) / 1_000, 3); } None => out.push(b'-') },
                Field::UpstreamStatus => match entry.backend { Some(_) => Self::number(out, u64::from(entry.status)), None => out.push(b'-') },
                Field::UpstreamAttempts => match entry.backend { Some(_) => Self::number(out, u64::from(entry.attempts)), None => out.push(b'-') },
                Field::Route => out.extend_from_slice(entry.route.as_deref().unwrap_or("-").as_bytes()),
                Field::RequestId => out.extend_from_slice(Self::dash(entry.request_id())),
                Field::Scheme => out.extend_from_slice(if entry.secure { b"https" } else { b"http" }),
                Field::Capture(index) => Self::quoted(out, Self::dash(entry.capture(index))),
            }

        }

    }

    fn iso8601 ( out: &mut Vec<u8>, now_ms: u64 ) {

        let seconds = now_ms / 1_000;
        let rest = seconds % 86_400;
        let ( year, month, day ) = Clock::civil((seconds / 86_400) as i64);

        let _ = write!(out, "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}+00:00", rest / 3_600, (rest % 3_600) / 60, rest % 60);

    }

}

impl Entry {

    pub fn capture ( &self, index: usize ) -> &[u8] {

        let end = self.cuts.iter().nth(index).copied().map_or(0, usize::from);
        let start = index.checked_sub(1).and_then(|previous| self.cuts.iter().nth(previous)).copied().map_or(0, usize::from);

        &self.captured[start.min(end)..end]

    }

}
