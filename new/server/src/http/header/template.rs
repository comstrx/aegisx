use http::header::HeaderValue;

use crate::core::error::{AppError, AppResult};
use super::arch::{Piece, Template, Var};

impl Var {

    pub fn named ( name: &str ) -> Option<Self> {

        Some(match name {
            "remote_addr" => Self::RemoteAddr,
            "remote_port" => Self::RemotePort,
            "host" => Self::Host,
            "scheme" => Self::Scheme,
            "request_id" => Self::RequestId,
            "request_uri" => Self::RequestUri,
            "uri" => Self::Uri,
            "args" => Self::Args,
            "upstream_addr" => Self::UpstreamAddr,
            "server_port" => Self::ServerPort,
            "msec" => Self::Msec,
            "ssl_client_verify" => Self::SslClientVerify,
            "ssl_client_s_dn" => Self::SslClientSDn,
            "ssl_client_i_dn" => Self::SslClientIDn,
            "ssl_client_serial" => Self::SslClientSerial,
            "ssl_client_fingerprint" => Self::SslClientFingerprint,
            _ => return None,
        })

    }

}

impl Template {

    pub fn compile ( text: &str ) -> AppResult<Self> {

        Self::compile_with(text, &|_| None)

    }

    pub fn compile_with ( text: &str, derived: &dyn Fn(&str) -> Option<usize> ) -> AppResult<Self> {

        let mut pieces = Vec::new();
        let mut literal = Vec::new();
        let bytes = text.as_bytes();
        let mut index = 0;

        while index < bytes.len() {

            if bytes[index] != b'$' { literal.push(bytes[index]); index += 1; continue; }

            let ( name, consumed ) = match bytes.get(index + 1) {
                Some(b'{') => {

                    let close = bytes[index..].iter().position(|byte| *byte == b'}').ok_or_else(|| AppError::config("headers", format!("unterminated variable in `{text}`")))?;

                    ( &text[index + 2..index + close], close + 1 )

                }
                Some(byte) if byte.is_ascii_alphabetic() || *byte == b'_' => {

                    let end = bytes[index + 1..].iter().position(|byte| !(byte.is_ascii_alphanumeric() || *byte == b'_')).map_or(bytes.len(), |offset| index + 1 + offset);

                    ( &text[index + 1..end], end - index )

                }
                _ => { literal.push(b'$'); index += 1; continue; }
            };

            let var = Var::named(name).or_else(|| derived(name).and_then(|index| u16::try_from(index).ok()).map(Var::Derived)).ok_or_else(|| AppError::config("headers", format!("unknown variable `${name}` in `{text}`")))?;

            if !literal.is_empty() { pieces.push(Piece::Text(std::mem::take(&mut literal).into_boxed_slice())); }

            pieces.push(Piece::Var(var));
            index += consumed;

        }

        if !literal.is_empty() { pieces.push(Piece::Text(literal.into_boxed_slice())); }

        Ok(Self { pieces })

    }

    pub fn is_static ( &self ) -> bool {

        self.pieces.iter().all(|piece| matches!(piece, Piece::Text(_)))

    }

    pub fn render ( &self, mut write: impl FnMut(Var, &mut Vec<u8>) ) -> Option<HeaderValue> {

        let mut out = Vec::with_capacity(64);

        for piece in &self.pieces {

            match piece {
                Piece::Text(text) => out.extend_from_slice(text),
                Piece::Var(var) => write(*var, &mut out),
            }

        }

        HeaderValue::from_bytes(&out).ok()

    }

}
