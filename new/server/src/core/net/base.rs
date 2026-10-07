use std::net::{IpAddr, SocketAddr};

use crate::core::error::{AppError, AppResult};
use super::arch::Addr;

impl Addr {

    pub fn parse ( text: &str ) -> AppResult<SocketAddr> {

        let text = text.trim();

        if let Ok(addr) = text.parse::<SocketAddr>() { return Ok(addr); }

        let ( host, port ) = text.rsplit_once(':').ok_or_else(|| AppError::invalid("address", format!("expected host:port, found `{text}`")))?;
        let port = port.parse::<u16>().map_err(|_| AppError::invalid("address", format!("invalid port in `{text}`")))?;

        let host = match host {
            "localhost" | "" => IpAddr::from([127, 0, 0, 1]),
            other => other.trim_matches(['[', ']']).parse().map_err(|_| AppError::invalid("address", format!("invalid host in `{text}`")))?,
        };

        Ok(SocketAddr::new(host, port))

    }

    pub fn is_loopback ( addr: &SocketAddr ) -> bool {

        addr.ip().is_loopback()

    }

}
