use std::borrow::Cow;

use super::arch::Request;

impl Request {

    pub fn canonical ( path: &str ) -> Option<Cow<'_, str>> {

        let bytes = path.as_bytes();

        if bytes.is_empty() || bytes[0] != b'/' { return None; }

        if memchr::memchr3(b'\\', b'%', b';', bytes).is_none() && !Self::has_dots(bytes) { return Some(Cow::Borrowed(path)); }

        let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
        let mut index = 0;

        while index < bytes.len() {

            let byte = bytes[index];

            match byte {
                b'\\' | b';' => return None,
                0..=0x20 | 0x7f => return None,
                b'%' => {

                    let high = *bytes.get(index + 1)?;
                    let low = *bytes.get(index + 2)?;
                    let value = (Self::hex(high)? << 4) | Self::hex(low)?;

                    if Self::unreserved(value) { out.push(value); }
                    else if value == b'/' || value == b'\\' || value == 0 || value == b'.' { return None; }
                    else { out.extend_from_slice(&bytes[index..index + 3]); }

                    index += 3;

                    continue;

                }
                other => out.push(other),
            }

            index += 1;

        }

        if Self::has_dots(&out) { return None; }

        if out == bytes { return Some(Cow::Borrowed(path)); }

        String::from_utf8(out).ok().map(Cow::Owned)

    }

    fn has_dots ( bytes: &[u8] ) -> bool {

        bytes.windows(2).any(|pair| pair == b"//" || pair == b"/.") || bytes.ends_with(b"/.") || bytes.ends_with(b"/..")

    }

    fn hex ( byte: u8 ) -> Option<u8> {

        match byte {
            b'0'..=b'9' => Some(byte - b'0'),
            b'a'..=b'f' => Some(byte - b'a' + 10),
            b'A'..=b'F' => Some(byte - b'A' + 10),
            _ => None,
        }

    }

    fn unreserved ( byte: u8 ) -> bool {

        byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~')

    }

    pub fn host_name ( host: &str ) -> &str {

        let host = host.trim();

        if host.starts_with('[') { return host.split(']').next().map(|value| &value[1..]).unwrap_or(host); }

        host.rsplit_once(':').filter(|( _, port )| port.bytes().all(|byte| byte.is_ascii_digit())).map(|( name, _ )| name).unwrap_or(host)

    }

    pub fn strip ( path: &str, prefix: usize ) -> &str {

        if prefix == 0 { return if path.is_empty() { "/" } else { path }; }

        path.get(prefix..).filter(|rest| rest.is_empty() || rest.starts_with('/')).map_or("/", |rest| if rest.is_empty() { "/" } else { rest })

    }

    pub fn decode ( path: &str ) -> Cow<'_, [u8]> {

        let bytes = path.as_bytes();

        if memchr::memchr(b'%', bytes).is_none() { return Cow::Borrowed(bytes); }

        let mut out = Vec::with_capacity(bytes.len());
        let mut index = 0;

        while index < bytes.len() {

            if bytes[index] == b'%' && let ( Some(high), Some(low) ) = ( bytes.get(index + 1).copied().and_then(Self::hex), bytes.get(index + 2).copied().and_then(Self::hex) ) {

                out.push((high << 4) | low);
                index += 3;

                continue;

            }

            out.push(bytes[index]);
            index += 1;

        }

        Cow::Owned(out)

    }

}
