mod headers;
pub use headers::Headers;

use std::net::IpAddr;
use openssl::{pkey::PKey, sign::Signer, hash::MessageDigest};
use uuid::Uuid;

use crate::core::{domain::Actor, error::{AppFail, AppResult}};
use crate::module::config::IdentityConfig;

pub struct Identity { key: PKey<openssl::pkey::Private> }

impl Identity {

    pub fn new ( key: Option<&[u8]> ) -> AppResult<Self> {

        Ok(Self { key: PKey::hmac(key.unwrap_or(Uuid::new_v4().as_bytes())).or_fail("Cannot initialize private identity key")? })

    }

    pub fn trusted ( config: &IdentityConfig, peer: IpAddr ) -> bool {

        config.trusted_peers.iter().any(|network| network.contains(&peer))

    }

    pub fn actor ( &self, config: &IdentityConfig, peer: IpAddr, claimed: Option<&str> ) -> Actor {

        let claimed = claimed.filter(|value| !value.is_empty() && value.len() <= 256 && !value.chars().any(char::is_control));
        let value = if Self::trusted(config, peer) && let Some(value) = claimed {
            format!("trusted:{value}")
        } else { format!("peer:{peer}") };
        let mut signer = Signer::new(MessageDigest::sha256(), &self.key).expect("SHA256 HMAC available");
        signer.update(value.as_bytes()).expect("in-memory HMAC");
        signer.sign_to_vec().expect("in-memory HMAC").try_into().expect("SHA256 width")

    }

}
