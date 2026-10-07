use std::io::Write;

use crate::core::net::Address;
use crate::core::time::Clock;
use super::arch::{Entry, Log};

const MONTHS: [&str; 12] = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];

impl Log {

    pub(super) fn combined ( out: &mut Vec<u8>, stamp: &[u8; 26], entry: &Entry ) {

        Self::address(out, entry.peer.ip());
        out.extend_from_slice(b" - - [");
        out.extend_from_slice(stamp);
        out.extend_from_slice(b"] \"");
        out.extend_from_slice(entry.method.as_str().as_bytes());
        out.push(b' ');
        out.extend_from_slice(entry.target());
        out.push(b' ');
        out.extend_from_slice(Self::version(entry.version));

        out.extend_from_slice(b"\" ");
        Self::number(out, u64::from(entry.status));
        out.push(b' ');
        Self::number(out, entry.sent() as u64);
        out.extend_from_slice(b" \"");
        Self::quoted(out, Self::dash(entry.referer()));
        out.extend_from_slice(b"\" \"");
        Self::quoted(out, Self::dash(entry.agent()));

        let elapsed = Clock::elapsed_us(entry.started);

        out.extend_from_slice(b"\" ");
        Self::number(out, elapsed / 1_000_000);
        out.push(b'.');
        Self::padded(out, elapsed % 1_000_000, 6);
        out.push(b' ');

        match &entry.backend { Some(addr) => Self::address_of(out, addr), None => out.push(b'-') }

        out.push(b' ');
        out.extend_from_slice(entry.route.as_deref().unwrap_or("-").as_bytes());
        out.push(b' ');
        out.extend_from_slice(Self::dash(entry.request_id()));
        out.push(b'\n');

    }

    pub(super) fn json ( out: &mut Vec<u8>, stamp: &[u8; 26], entry: &Entry ) {

        out.extend_from_slice(b"{\"time\":\"");
        out.extend_from_slice(stamp);

        out.extend_from_slice(b"\",\"peer\":\"");
        Self::address(out, entry.peer.ip());
        out.extend_from_slice(b"\",\"method\":\"");
        out.extend_from_slice(entry.method.as_str().as_bytes());
        out.extend_from_slice(b"\",\"target\":");
        Self::string(out, entry.target());
        out.extend_from_slice(b",\"version\":\"");
        out.extend_from_slice(Self::version(entry.version));
        out.extend_from_slice(b"\",\"status\":");
        Self::number(out, u64::from(entry.status));
        out.extend_from_slice(b",\"bytes_sent\":");
        Self::number(out, entry.sent() as u64);
        out.extend_from_slice(b",\"bytes_received\":");
        Self::number(out, entry.received);
        out.extend_from_slice(b",\"request_time_us\":");
        Self::number(out, Clock::elapsed_us(entry.started));
        out.extend_from_slice(b",\"referer\":");

        Self::optional(out, Some(entry.referer()).filter(|value| !value.is_empty()));
        out.extend_from_slice(b",\"user_agent\":");
        Self::optional(out, Some(entry.agent()).filter(|value| !value.is_empty()));
        out.extend_from_slice(b",\"backend\":");

        match &entry.backend { Some(addr) => { out.push(b'"'); Self::address_of(out, addr); out.push(b'"'); } None => out.extend_from_slice(b"null") }

        if entry.backend.is_some() {

            out.extend_from_slice(b",\"upstream_time_us\":");
            Self::number(out, entry.header_us);
            out.extend_from_slice(b",\"attempts\":");
            Self::number(out, u64::from(entry.attempts));

        }

        out.extend_from_slice(b",\"route\":");
        Self::optional(out, entry.route.as_deref().map(str::as_bytes));
        out.extend_from_slice(b",\"request_id\":");
        Self::optional(out, Some(entry.request_id()).filter(|value| !value.is_empty()));
        out.extend_from_slice(b"}\n");

    }

    pub(super) fn stamp ( &self, now_ms: u64 ) -> [u8; 26] {

        let seconds = now_ms / 1_000;
        let mut cached = self.stamp.borrow_mut();

        if cached.0 == seconds { return cached.1; }

        let rest = seconds % 86_400;
        let ( year, month, day ) = Clock::civil((seconds / 86_400) as i64);
        let name = MONTHS.get((month as usize).wrapping_sub(1)).copied().unwrap_or("Jan");
        let mut text = Vec::with_capacity(26);

        let _ = write!(text, "{day:02}/{name}/{year}:{:02}:{:02}:{:02} +0000", rest / 3_600, (rest % 3_600) / 60, rest % 60);

        let mut fixed = [b' '; 26];
        let length = text.len().min(26);

        fixed[..length].copy_from_slice(&text[..length]);
        *cached = ( seconds, fixed );

        fixed

    }

    pub(super) fn number ( out: &mut Vec<u8>, mut value: u64 ) {

        let mut digits = [0u8; 20];
        let mut at = digits.len();

        loop {

            at -= 1;
            digits[at] = b'0' + (value % 10) as u8;
            value /= 10;

            if value == 0 { break; }

        }

        out.extend_from_slice(&digits[at..]);

    }

    pub(super) fn padded ( out: &mut Vec<u8>, mut value: u64, width: usize ) {

        let mut digits = [b'0'; 20];
        let mut at = digits.len();

        while at > digits.len() - width { at -= 1; digits[at] = b'0' + (value % 10) as u8; value /= 10; }

        out.extend_from_slice(&digits[at..]);

    }

    pub(super) fn address ( out: &mut Vec<u8>, ip: std::net::IpAddr ) {

        match ip {
            std::net::IpAddr::V4(v4) => {

                for ( index, octet ) in v4.octets().iter().enumerate() {

                    if index > 0 { out.push(b'.'); }

                    Self::number(out, u64::from(*octet));

                }

            }
            std::net::IpAddr::V6(v6) => { let _ = write!(out, "{v6}"); }
        }

    }

    pub(super) fn address_of ( out: &mut Vec<u8>, addr: &Address ) {

        match addr {
            Address::Tcp(socket) => Self::socket(out, *socket),
            Address::Unix(path) => { out.extend_from_slice(b"unix:"); out.extend_from_slice(path.as_bytes()); }
            Address::Name(host, port) => { out.extend_from_slice(host.as_bytes()); out.push(b':'); Self::number(out, u64::from(*port)); }
        }

    }

    fn socket ( out: &mut Vec<u8>, addr: std::net::SocketAddr ) {

        match addr {
            std::net::SocketAddr::V4(v4) => { Self::address(out, std::net::IpAddr::V4(*v4.ip())); out.push(b':'); Self::number(out, u64::from(v4.port())); }
            std::net::SocketAddr::V6(_) => { let _ = write!(out, "{addr}"); }
        }

    }

    pub(super) fn dash ( value: &[u8] ) -> &[u8] {

        if value.is_empty() { b"-" } else { value }

    }

    pub(super) fn version ( version: http::Version ) -> &'static [u8] {

        match version {
            http::Version::HTTP_09 => b"HTTP/0.9",
            http::Version::HTTP_10 => b"HTTP/1.0",
            http::Version::HTTP_11 => b"HTTP/1.1",
            http::Version::HTTP_2 => b"HTTP/2.0",
            http::Version::HTTP_3 => b"HTTP/3.0",
            _ => b"HTTP/?",
        }

    }

    pub(super) fn quoted ( out: &mut Vec<u8>, bytes: &[u8] ) {

        for byte in bytes {

            match byte {
                b'"' => out.extend_from_slice(b"\\\""),
                b'\\' => out.extend_from_slice(b"\\\\"),
                0x20..=0x7e => out.push(*byte),
                _ => { let _ = write!(out, "\\x{byte:02X}"); }
            }

        }

    }

    fn optional ( out: &mut Vec<u8>, bytes: Option<&[u8]> ) {

        match bytes { Some(bytes) => Self::string(out, bytes), None => out.extend_from_slice(b"null") }

    }

    fn string ( out: &mut Vec<u8>, bytes: &[u8] ) {

        out.push(b'"');

        for byte in bytes {

            match byte {
                b'"' => out.extend_from_slice(b"\\\""),
                b'\\' => out.extend_from_slice(b"\\\\"),
                b'\n' => out.extend_from_slice(b"\\n"),
                b'\r' => out.extend_from_slice(b"\\r"),
                b'\t' => out.extend_from_slice(b"\\t"),
                0x20..=0x7e => out.push(*byte),
                0x80..=0xff => out.push(*byte),
                _ => { let _ = write!(out, "\\u{byte:04x}"); }
            }

        }

        out.push(b'"');

    }

}
