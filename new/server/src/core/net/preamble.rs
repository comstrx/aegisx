use std::net::SocketAddr;

use ppp::{HeaderResult, PartialResult, v1, v2};

use super::arch::{Announced, BINARY, Preamble, TEXT, Wire};

impl Preamble {

    pub fn read ( bytes: &[u8] ) -> Announced {

        let begins = |signature: &[u8]| { let seen = bytes.len().min(signature.len()); bytes[..seen] == signature[..seen] };

        if !begins(TEXT) && !begins(BINARY) { return Announced::Invalid; }

        match HeaderResult::parse(bytes) {
            HeaderResult::V1(Ok(header)) => Announced::Done {
                length : header.header.len(),
                source : match header.addresses {
                    v1::Addresses::Tcp4(route) => Some(SocketAddr::from(( route.source_address, route.source_port ))),
                    v1::Addresses::Tcp6(route) => Some(SocketAddr::from(( route.source_address, route.source_port ))),
                    v1::Addresses::Unknown => None,
                },
            },
            HeaderResult::V2(Ok(header)) => Announced::Done {
                length : header.len(),
                source : match ( header.command, header.addresses ) {
                    ( v2::Command::Proxy, v2::Addresses::IPv4(route) ) => Some(SocketAddr::from(( route.source_address, route.source_port ))),
                    ( v2::Command::Proxy, v2::Addresses::IPv6(route) ) => Some(SocketAddr::from(( route.source_address, route.source_port ))),
                    _ => None,
                },
            },
            partial if partial.is_incomplete() => Announced::Partial,
            _ => Announced::Invalid,
        }

    }

    pub fn write ( wire: Wire, source: SocketAddr, destination: SocketAddr ) -> Vec<u8> {

        match wire {
            Wire::V1 => match ( source, destination ) {
                ( SocketAddr::V4(source), SocketAddr::V4(destination) ) => format!("PROXY TCP4 {} {} {} {}\r\n", source.ip(), destination.ip(), source.port(), destination.port()).into_bytes(),
                ( SocketAddr::V6(source), SocketAddr::V6(destination) ) => format!("PROXY TCP6 {} {} {} {}\r\n", source.ip(), destination.ip(), source.port(), destination.port()).into_bytes(),
                _ => b"PROXY UNKNOWN\r\n".to_vec(),
            },
            Wire::V2 => v2::Builder::with_addresses(v2::Version::Two | v2::Command::Proxy, v2::Protocol::Stream, ( source, destination )).build().unwrap_or_default(),
        }

    }

}
