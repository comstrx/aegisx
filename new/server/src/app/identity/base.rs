use std::net::{IpAddr, SocketAddr};

use http::header::{HeaderMap, HeaderName, HeaderValue};

use crate::app::Snapshot;
use crate::http::proxy::Extras;
use crate::core::arena::Arena;
use crate::core::rand::Rng;
use super::arch::{Certificate, Identity, Peer};

impl Identity {

    pub fn peer ( snapshot: &Snapshot, addr: SocketAddr, tls: bool ) -> Peer {

        let trusted = snapshot.trusted.contains(addr.ip());
        let forward = HeaderValue::from_str(&addr.ip().to_string()).unwrap_or_else(|_| HeaderValue::from_static("unknown"));
        let proto = HeaderValue::from_static(if tls { "https" } else { "http" });

        Peer { addr, trusted, forward, proto }

    }

    pub fn certificate ( der: &[u8] ) -> Option<Certificate> {

        use std::fmt::Write;

        let ( _, parsed ) = x509_parser::parse_x509_certificate(der).ok()?;
        let digest = aws_lc_rs::digest::digest(&aws_lc_rs::digest::SHA256, der);
        let mut fingerprint = String::with_capacity(64);
        let mut serial = String::with_capacity(parsed.raw_serial().len() * 2);

        for byte in digest.as_ref() { let _ = write!(fingerprint, "{byte:02x}"); }

        for byte in parsed.raw_serial() { let _ = write!(serial, "{byte:02X}"); }

        Some(Certificate { subject: parsed.subject().to_string().into(), issuer: parsed.issuer().to_string().into(), serial: serial.into(), fingerprint: fingerprint.into() })

    }

    pub fn request_id ( rng: &mut Rng, arena: &mut Arena ) -> HeaderValue {

        let mut text = [0u8; 36];

        rng.uuid(&mut text);

        HeaderValue::from_maybe_shared(arena.copy(&text)).unwrap_or_else(|_| HeaderValue::from_static("00000000-0000-4000-8000-000000000000"))

    }

    pub fn trace ( rng: &mut Rng ) -> Option<HeaderValue> {

        HeaderValue::from_str(&format!("00-{:032x}-{:016x}-01", rng.next_u128() | 1, rng.next_u64() | 1)).ok()

    }

    pub fn claim ( headers: &HeaderMap, snapshot: &Snapshot, peer: &Peer, rng: &mut Rng, arena: &mut Arena ) -> HeaderValue {

        if peer.trusted && let Some(value) = headers.get(&snapshot.names.request_id) && Self::looks_like_uuid(value.as_bytes()) {

            return value.clone();

        }

        Self::request_id(rng, arena)

    }

    pub fn chain ( headers: &HeaderMap, snapshot: &Snapshot, peer: &Peer ) -> Option<HeaderValue> {

        if !snapshot.config.identity.forwarding || !peer.trusted { return None; }

        let chain = headers.get(&snapshot.names.forwarded_for).and_then(|value| value.to_str().ok()).filter(|chain| !chain.is_empty())?;

        HeaderValue::from_str(&format!("{chain}, {}", peer.forward.to_str().unwrap_or("unknown"))).ok()

    }

    pub fn outgoing <'a> ( snapshot: &'a Snapshot, peer: &'a Peer, chain: Option<&'a HeaderValue>, id: &'a HeaderValue ) -> Extras<'a> {

        let names = &snapshot.names;
        let settings = &snapshot.config.identity;

        [
            settings.forwarding.then_some(( &names.forwarded_for, chain.unwrap_or(&peer.forward) )),
            settings.forwarding.then_some(( &names.forwarded_proto, &peer.proto )),
            settings.propagate.then_some(( &names.request_id, id )),
        ]

    }

    pub fn client ( headers: &HeaderMap, snapshot: &Snapshot, peer: &Peer ) -> IpAddr {

        if !peer.trusted { return peer.addr.ip(); }

        headers.get(&snapshot.names.forwarded_for).and_then(|value| value.to_str().ok())
            .and_then(|chain| chain.rsplit(',').filter_map(|hop| hop.trim().parse::<IpAddr>().ok()).find(|ip| !snapshot.trusted.contains(*ip)))
            .unwrap_or_else(|| peer.addr.ip())

    }

    pub fn drops <'s> ( snapshot: &'s Snapshot, peer: &Peer ) -> &'s [HeaderName] {

        if peer.trusted { &snapshot.names.drop_trusted } else { &snapshot.names.drop_untrusted }

    }

    pub fn echo ( headers: &mut HeaderMap, snapshot: &Snapshot, id: &HeaderValue ) {

        if snapshot.config.identity.propagate { headers.insert(snapshot.names.request_id.clone(), id.clone()); }

    }

    pub fn looks_like_uuid ( value: &[u8] ) -> bool {

        value.len() == 36 && value.iter().enumerate().all(|( index, byte )| match index {
            8 | 13 | 18 | 23 => *byte == b'-',
            _ => byte.is_ascii_hexdigit(),
        })

    }

}
