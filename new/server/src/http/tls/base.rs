use std::collections::HashMap;
use std::net::IpAddr;
use std::path::Path;
use std::sync::Arc;

use rustls::pki_types::pem::PemObject;
use rustls::pki_types::{CertificateDer, PrivateKeyDer, ServerName};
use rustls::crypto::aws_lc_rs;
use rustls::server::{ClientHello, NoServerSessionStorage, ResolvesServerCert, ServerSessionMemoryCache, WebPkiClientVerifier};
use rustls::sign::CertifiedKey;
use rustls::{ClientConfig, RootCertStore, ServerConfig};
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::net::TcpStream;
use tokio_rustls::{LazyConfigAcceptor, TlsAcceptor, TlsConnector, client, server};

use crate::core::error::{AppError, AppResult};
use crate::core::rt::Rt;
use super::arch::{ACME_ALPN, Acceptor, Bundle, Policy, Resolver, Tls, Trust};

const OBTAIN_MS: u64 = 90_000;

pub const ALPN_HTTP1: &[u8] = b"http/1.1";
pub const ALPN_HTTP2: &[u8] = b"h2";

impl Tls {

    pub fn acceptor <'a> ( default: Bundle<'a>, named: impl IntoIterator<Item = Bundle<'a>>, policy: Policy<'a> ) -> AppResult<Acceptor> {

        let Policy { timeout_ms, http2, resumption, client, challenges, demand, obtain, modern } = policy;
        let versions: &[&rustls::SupportedProtocolVersion] = if modern { &[&rustls::version::TLS13] } else { rustls::ALL_VERSIONS };

        let default = Arc::new(match demand.as_ref().filter(|demand| demand.local() && default.cert.as_os_str().is_empty()) {
            Some(demand) => demand.authority.as_ref().map(|authority| authority.leaf(&["localhost".to_string()])).transpose()?.ok_or_else(|| AppError::config("set_tls", "the local authority is not available"))?,
            None => Self::identity("set_tls", default.cert, default.key, default.ocsp)?,
        });
        let mut exact = HashMap::new();
        let mut wildcard = Vec::new();

        for certificate in named {

            let identity = Arc::new(Self::identity("add_certificate", certificate.cert, certificate.key, certificate.ocsp)?);

            for name in certificate.names {

                match name.strip_prefix("*.") {
                    Some(suffix) => wildcard.push(( format!(".{}", suffix.to_ascii_lowercase()), identity.clone() )),
                    None => { exact.insert(name.to_ascii_lowercase(), identity.clone()); }
                }

            }

        }

        wildcard.sort_by_key(|( suffix, _ )| std::cmp::Reverse(suffix.len()));

        let acme = challenges.is_some();
        let resolver = Arc::new(Resolver { default, exact, wildcard, challenges, demand });
        let mut config = match client {
            Some(policy) => {

                let mut roots = RootCertStore::empty();

                for cert in Self::chain("set_tls", policy.ca)? { roots.add(cert).map_err(|error| AppError::config("set_tls", format!("invalid client ca certificate in {}: {error}", policy.ca.display())))?; }

                let builder = WebPkiClientVerifier::builder(Arc::new(roots));
                let builder = if policy.required { builder } else { builder.allow_unauthenticated() };
                let verifier = builder.build().map_err(|error| AppError::config("set_tls", format!("client ca is not usable: {error}")))?;

                ServerConfig::builder_with_protocol_versions(versions).with_client_cert_verifier(verifier).with_cert_resolver(resolver)

            }
            None => ServerConfig::builder_with_protocol_versions(versions).with_no_client_auth().with_cert_resolver(resolver),
        };

        config.alpn_protocols = if http2 { vec![ALPN_HTTP2.to_vec(), ALPN_HTTP1.to_vec()] } else { vec![ALPN_HTTP1.to_vec()] };

        if acme { config.alpn_protocols.push(ACME_ALPN.to_vec()); }

        config.session_storage = if resumption.sessions == 0 { Arc::new(NoServerSessionStorage {}) } else { ServerSessionMemoryCache::new(resumption.sessions) };

        if resumption.tickets { config.ticketer = aws_lc_rs::Ticketer::new().map_err(|error| AppError::config("set_tls", format!("cannot create session ticketer: {error}")))?; }

        Ok(Acceptor { config: Arc::new(config), timeout_ms, obtain })

    }

    pub fn identity ( key: &str, cert: &Path, private: &Path, ocsp: Option<&Path> ) -> AppResult<CertifiedKey> {

        let chain = Self::chain(key, cert)?;
        let private = PrivateKeyDer::from_pem_file(private).map_err(|error| AppError::config(key, format!("cannot load key {}: {error}", private.display())))?;
        let mut certified = CertifiedKey::from_der(chain, private, &aws_lc_rs::default_provider()).map_err(|error| AppError::config(key, format!("certificate and key do not form a valid identity: {error}")))?;

        if let Some(path) = ocsp { certified.ocsp = Some(std::fs::read(path).map_err(|error| AppError::config(key, format!("cannot read ocsp response {}: {error}", path.display())))?); }

        Ok(certified)

    }

    pub fn server_name ( hello: &[u8] ) -> Option<Option<String>> {

        let mut acceptor = rustls::server::Acceptor::default();
        let mut bytes = hello;

        if acceptor.read_tls(&mut bytes).is_err() { return Some(None); }

        match acceptor.accept() {
            Ok(Some(accepted)) => Some(accepted.client_hello().server_name().map(str::to_ascii_lowercase)),
            Ok(None) => None,
            Err(_) => Some(None),
        }

    }

    pub fn trust ( server_name: &str, ca: Option<&Path>, client: Option<( &Path, &Path )>, protocols: &[&[u8]] ) -> AppResult<Trust> {

        let mut roots = RootCertStore::empty();

        match ca {
            Some(path) => {

                for cert in Self::chain("add_upstream", path)? {

                    roots.add(cert).map_err(|error| AppError::config("add_upstream", format!("invalid ca certificate in {}: {error}", path.display())))?;

                }

            }
            None => roots.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned()),
        }

        let name = ServerName::try_from(server_name.to_string()).map_err(|_| AppError::config("add_upstream", format!("invalid server_name `{server_name}`")))?;
        let builder = ClientConfig::builder().with_root_certificates(roots);

        let mut config = match client {
            Some(( cert, key )) => {

                let private = PrivateKeyDer::from_pem_file(key).map_err(|error| AppError::config("add_upstream", format!("cannot load key {}: {error}", key.display())))?;

                builder.with_client_auth_cert(Self::chain("add_upstream", cert)?, private).map_err(|error| AppError::config("add_upstream", format!("invalid client certificate {}: {error}", cert.display())))?

            }
            None => builder.with_no_client_auth(),
        };

        config.alpn_protocols = protocols.iter().map(|protocol| protocol.to_vec()).collect();
        config.resumption = rustls::client::Resumption::in_memory_sessions(1_024);

        Ok(Trust { config: Arc::new(config), name })

    }

    fn chain ( key: &str, path: &Path ) -> AppResult<Vec<CertificateDer<'static>>> {

        let certs = CertificateDer::pem_file_iter(path).map_err(|error| AppError::config(key, format!("cannot read {}: {error}", path.display())))?;
        let mut chain = Vec::new();

        for cert in certs {

            chain.push(cert.map_err(|error| AppError::config(key, format!("invalid certificate in {}: {error}", path.display())))?);

        }

        if chain.is_empty() { return Err(AppError::config(key, format!("{} holds no certificates", path.display()))); }

        Ok(chain)

    }

}

impl Acceptor {

    pub fn quic ( &self ) -> Arc<ServerConfig> {

        let mut config = (*self.config).clone();

        config.alpn_protocols = vec![b"h3".to_vec()];
        config.max_early_data_size = 0;

        Arc::new(config)

    }

    pub async fn accept <S: AsyncRead + AsyncWrite + Unpin> ( &self, stream: S ) -> AppResult<server::TlsStream<S>> {

        let fail = |error: std::io::Error| AppError::network("tls handshake", error.to_string());

        let Some(obtain) = &self.obtain else {

            return Rt::timeout("tls handshake", self.timeout_ms, TlsAcceptor::from(self.config.clone()).accept(stream)).await?.map_err(fail);

        };

        let start = Rt::timeout("tls handshake", self.timeout_ms, LazyConfigAcceptor::new(rustls::server::Acceptor::default(), stream)).await?.map_err(fail)?;
        let wanted = { let hello = start.client_hello(); hello.server_name().filter(|_| !hello.alpn().is_some_and(|mut protocols| protocols.any(|protocol| protocol == ACME_ALPN))).map(str::to_ascii_lowercase) };

        if let Some(name) = wanted { let _ = Rt::timeout("certificate on demand", OBTAIN_MS, obtain.obtain(name)).await; }

        Rt::timeout("tls handshake", self.timeout_ms, start.into_stream(self.config.clone())).await?.map_err(fail)

    }

}

impl Trust {

    pub fn host ( &self ) -> String {

        match &self.name {
            ServerName::DnsName(name) => name.as_ref().to_string(),
            ServerName::IpAddress(ip) => match IpAddr::from(*ip) {
                IpAddr::V6(ip) => format!("[{ip}]"),
                IpAddr::V4(ip) => ip.to_string(),
            },
            _ => self.name.to_str().into_owned(),
        }

    }

    pub async fn connect ( &self, stream: TcpStream ) -> AppResult<client::TlsStream<TcpStream>> {

        let connector = TlsConnector::from(self.config.clone());

        connector.connect(self.name.clone(), stream).await
            .map_err(|error| AppError::network(self.name.to_str(), error.to_string()))

    }

}

impl ResolvesServerCert for Resolver {

    fn resolve ( &self, hello: ClientHello<'_> ) -> Option<Arc<CertifiedKey>> {

        if let Some(challenges) = &self.challenges && hello.alpn().is_some_and(|mut protocols| protocols.any(|protocol| protocol == ACME_ALPN)) {

            return challenges.read().ok()?.get(hello.server_name()?).cloned();

        }

        let Some(name) = hello.server_name() else { return Some(self.default.clone()); };

        if let Some(identity) = self.exact.get(name) { return Some(identity.clone()); }

        if let Some(( _, identity )) = self.wildcard.iter().find(|( suffix, _ )| name.len() > suffix.len() && name.ends_with(suffix.as_str()) && !name[..name.len() - suffix.len()].contains('.')) {

            return Some(identity.clone());

        }

        if let Some(identity) = self.demand.as_ref().and_then(|demand| demand.find(name)) { return Some(identity); }

        Some(self.default.clone())

    }

}
