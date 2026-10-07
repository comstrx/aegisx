use std::sync::Arc;

use rustls::pki_types::ServerName;
use rustls::{ClientConfig, ServerConfig};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Tls;

pub const ACME_ALPN: &[u8] = b"acme-tls/1";

pub type Challenges = Arc<std::sync::RwLock<std::collections::HashMap<String, Arc<rustls::sign::CertifiedKey>>>>;

pub type Held = Arc<std::sync::RwLock<std::collections::HashMap<String, Minted>>>;

#[derive(Clone)]
pub struct Minted {
    pub identity : Arc<rustls::sign::CertifiedKey>,
    pub renew    : u64,
}

pub struct Authority {
    pub(super) issuer : rcgen::Issuer<'static, rcgen::KeyPair>,
    pub(super) days   : u32,
}

pub struct Demand {
    pub(super) names     : Vec<String>,
    pub(super) held      : Held,
    pub(super) capacity  : usize,
    pub(super) authority : Option<Authority>,
}

pub trait Obtain: Send + Sync {
    fn obtain ( &self, name: String ) -> std::pin::Pin<Box<dyn Future<Output = ()>>>;
}

#[derive(Clone, Copy, Debug)]
pub struct Bundle <'a> {
    pub names : &'a [String],
    pub cert  : &'a std::path::Path,
    pub key   : &'a std::path::Path,
    pub ocsp  : Option<&'a std::path::Path>,
}

pub struct Policy <'a> {
    pub timeout_ms : u64,
    pub http2      : bool,
    pub resumption : Resumption,
    pub client     : Option<ClientPolicy<'a>>,
    pub challenges : Option<Challenges>,
    pub demand     : Option<Arc<Demand>>,
    pub obtain     : Option<Arc<dyn Obtain>>,
    pub modern     : bool,
}

#[derive(Debug)]
pub struct Resolver {
    pub(super) default    : Arc<rustls::sign::CertifiedKey>,
    pub(super) exact      : std::collections::HashMap<String, Arc<rustls::sign::CertifiedKey>>,
    pub(super) wildcard   : Vec<( String, Arc<rustls::sign::CertifiedKey> )>,
    pub(super) challenges : Option<Challenges>,
    pub(super) demand     : Option<Arc<Demand>>,
}

#[derive(Clone, Copy, Debug)]
pub struct ClientPolicy <'a> {
    pub ca       : &'a std::path::Path,
    pub required : bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Resumption {
    pub sessions : usize,
    pub tickets  : bool,
}

#[derive(Clone)]
pub struct Acceptor {
    pub(super) config     : Arc<ServerConfig>,
    pub(super) timeout_ms : u64,
    pub(super) obtain     : Option<Arc<dyn Obtain>>,
}

#[derive(Clone, Debug)]
pub struct Trust {
    pub(super) config : Arc<ClientConfig>,
    pub(super) name   : ServerName<'static>,
}
