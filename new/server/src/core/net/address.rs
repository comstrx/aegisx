use std::fmt;
use std::net::{IpAddr, SocketAddr};
use std::str::FromStr;

use serde::de::{Deserialize, Deserializer, Error as _};

use crate::core::error::{AppError, AppResult};
use super::arch::{Addr, Address};

impl Address {

    pub fn parse ( text: &str ) -> AppResult<Self> {

        let text = text.trim();

        match text.strip_prefix("unix:") {
            Some(path) if cfg!(unix) && !path.is_empty() => Ok(Self::Unix(path.into())),
            Some(_) if cfg!(unix) => Err(AppError::invalid("address", "unix socket path is empty")),
            Some(_) => Err(AppError::unsupported("unix sockets on this platform")),
            None => Addr::parse(text).map(Self::Tcp).or_else(|error| Self::named(text).ok_or(error)),
        }

    }

    fn named ( text: &str ) -> Option<Self> {

        let ( host, port ) = text.rsplit_once(':')?;
        let port = port.parse::<u16>().ok()?;
        let host = host.trim_end_matches('.');

        if host.is_empty() || host.len() > 253 || !host.split('.').all(|label| !label.is_empty() && label.len() <= 63 && label.bytes().all(|byte| byte.is_ascii_alphanumeric() || byte == b'-') && !label.starts_with('-') && !label.ends_with('-')) { return None; }

        Some(Self::Name(host.to_ascii_lowercase().into(), port))

    }

    pub fn name ( &self ) -> Option<( &str, u16 )> {

        match self { Self::Name(host, port) => Some(( host, *port )), Self::Tcp(_) | Self::Unix(_) => None }

    }

    pub fn socket ( &self ) -> Option<SocketAddr> {

        match self { Self::Tcp(addr) => Some(*addr), Self::Unix(_) | Self::Name(..) => None }

    }

    pub fn ip ( &self ) -> Option<IpAddr> {

        self.socket().map(|addr| addr.ip())

    }

    pub fn port ( &self ) -> Option<u16> {

        match self { Self::Tcp(addr) => Some(addr.port()), Self::Name(_, port) => Some(*port), Self::Unix(_) => None }

    }

    pub fn is_unix ( &self ) -> bool {

        matches!(self, Self::Unix(_))

    }

    pub fn path ( &self ) -> Option<&str> {

        match self { Self::Unix(path) => Some(path), Self::Tcp(_) | Self::Name(..) => None }

    }

}

impl fmt::Display for Address {

    fn fmt ( &self, f: &mut fmt::Formatter<'_> ) -> fmt::Result {

        match self { Self::Tcp(addr) => write!(f, "{addr}"), Self::Unix(path) => write!(f, "unix:{path}"), Self::Name(host, port) => write!(f, "{host}:{port}") }

    }

}

impl FromStr for Address {

    type Err = AppError;

    fn from_str ( text: &str ) -> Result<Self, AppError> {

        Self::parse(text)

    }

}

impl From<SocketAddr> for Address {

    fn from ( addr: SocketAddr ) -> Self {

        Self::Tcp(addr)

    }

}

impl Default for Address {

    fn default () -> Self {

        Self::Tcp(SocketAddr::from(( [127, 0, 0, 1], 0 )))

    }

}

impl<'de> Deserialize<'de> for Address {

    fn deserialize <D: Deserializer<'de>> ( deserializer: D ) -> Result<Self, D::Error> {

        let text = String::deserialize(deserializer)?;

        Self::parse(&text).map_err(D::Error::custom)

    }

}
