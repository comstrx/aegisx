use std::net::IpAddr;
use std::path::Path;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use rcgen::{BasicConstraints, CertificateParams, DistinguishedName, DnType, ExtendedKeyUsagePurpose, IsCa, Issuer, KeyPair, KeyUsagePurpose};
use rustls::crypto::aws_lc_rs;
use rustls::pki_types::{PrivateKeyDer, PrivatePkcs8KeyDer};
use rustls::sign::CertifiedKey;

use crate::core::error::{AppError, AppResult};
use super::arch::{Authority, Demand, Held, Minted};

const ROOT_DAYS: i64 = 3_650;
const ROOT_NAME: &str = "AegisX Local Authority";

impl Authority {

    pub fn open ( dir: &Path, days: u32 ) -> AppResult<Self> {

        let fail = |what: &str, error: String| AppError::config("set_tls", format!("local authority in {}: {what}: {error}", dir.display()));
        let ( cert, key ) = ( dir.join("root.pem"), dir.join("root.key") );

        if !(cert.is_file() && key.is_file()) {

            std::fs::create_dir_all(dir).map_err(|error| fail("cannot create the directory", error.to_string()))?;

            let pair = KeyPair::generate().map_err(|error| fail("cannot generate the root key", error.to_string()))?;
            let mut params = CertificateParams::new(Vec::<String>::new()).map_err(|error| fail("invalid root", error.to_string()))?;

            params.is_ca = IsCa::Ca(BasicConstraints::Constrained(0));
            params.distinguished_name = DistinguishedName::new();
            params.distinguished_name.push(DnType::CommonName, ROOT_NAME);
            params.key_usages = vec![KeyUsagePurpose::KeyCertSign, KeyUsagePurpose::CrlSign];
            params.not_before = time::OffsetDateTime::now_utc() - time::Duration::days(1);
            params.not_after = time::OffsetDateTime::now_utc() + time::Duration::days(ROOT_DAYS);

            let root = params.self_signed(&pair).map_err(|error| fail("cannot sign the root", error.to_string()))?;

            std::fs::write(&key, pair.serialize_pem()).map_err(|error| fail("cannot write root.key", error.to_string()))?;

            #[cfg(unix)]
            { use std::os::unix::fs::PermissionsExt; let _ = std::fs::set_permissions(&key, std::fs::Permissions::from_mode(0o600)); }

            std::fs::write(&cert, root.pem()).map_err(|error| fail("cannot write root.pem", error.to_string()))?;

        }

        let pair = KeyPair::from_pem(&std::fs::read_to_string(&key).map_err(|error| fail("cannot read root.key", error.to_string()))?).map_err(|error| fail("root.key is not a key", error.to_string()))?;
        let issuer = Issuer::from_ca_cert_pem(&std::fs::read_to_string(&cert).map_err(|error| fail("cannot read root.pem", error.to_string()))?, pair).map_err(|error| fail("root.pem is not a certificate authority", error.to_string()))?;

        Ok(Self { issuer, days: days.max(1) })

    }

    pub fn leaf ( &self, names: &[String] ) -> AppResult<CertifiedKey> {

        let fail = |error: String| AppError::message(format!("local authority cannot issue for {names:?}: {error}"));
        let key = KeyPair::generate().map_err(|error| fail(error.to_string()))?;
        let mut params = CertificateParams::new(names.to_vec()).map_err(|error| fail(error.to_string()))?;

        params.distinguished_name = DistinguishedName::new();
        params.key_usages = vec![KeyUsagePurpose::DigitalSignature];
        params.extended_key_usages = vec![ExtendedKeyUsagePurpose::ServerAuth];
        params.not_before = time::OffsetDateTime::now_utc() - time::Duration::hours(1);
        params.not_after = time::OffsetDateTime::now_utc() + time::Duration::days(i64::from(self.days));

        let cert = params.signed_by(&key, &self.issuer).map_err(|error| fail(error.to_string()))?;
        let private = PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(key.serialize_der()));
        let signing = aws_lc_rs::sign::any_supported_type(&private).map_err(|error| fail(error.to_string()))?;

        Ok(CertifiedKey::new(vec![cert.der().clone()], signing))

    }

}

impl Demand {

    pub fn new ( names: Vec<String>, capacity: usize, authority: Option<Authority>, held: Held ) -> Self {

        Self { names: names.into_iter().map(|name| name.to_ascii_lowercase()).collect(), held, capacity: capacity.max(1), authority }

    }

    pub fn now () -> u64 {

        SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |since| since.as_secs())

    }

    pub fn local ( &self ) -> bool {

        self.authority.is_some()

    }

    pub fn permits ( &self, name: &str ) -> bool {

        let host = !name.is_empty() && name.len() <= 253 && name.parse::<IpAddr>().is_err() && name.split('.').all(|label| !label.is_empty() && label.len() <= 63 && !label.starts_with('-') && !label.ends_with('-') && label.bytes().all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-'));

        host && self.names.iter().any(|pattern| match pattern.strip_prefix("*.") {
            Some(suffix) => name.strip_suffix(suffix).and_then(|head| head.strip_suffix('.')).is_some_and(|head| !head.is_empty() && !head.contains('.')),
            None => pattern == "*" || pattern == name,
        })

    }

    pub fn held ( &self, name: &str ) -> Option<Minted> {

        self.held.read().ok()?.get(name).cloned()

    }

    pub fn due ( &self, now: u64 ) -> Vec<String> {

        self.held.read().map_or_else(|_| Vec::new(), |held| held.iter().filter(|( _, minted )| minted.renew <= now).map(|( name, _ )| name.clone()).collect())

    }

    pub fn install ( &self, name: &str, identity: Arc<CertifiedKey>, renew: u64 ) -> bool {

        let Ok(mut held) = self.held.write() else { return false; };

        if held.len() >= self.capacity && !held.contains_key(name) { return false; }

        held.insert(name.to_string(), Minted { identity, renew });

        true

    }

    pub fn find ( &self, name: &str ) -> Option<Arc<CertifiedKey>> {

        let lowered;
        let name = if name.bytes().any(|byte| byte.is_ascii_uppercase()) { lowered = name.to_ascii_lowercase(); lowered.as_str() } else { name };
        let now = Self::now();

        if let Some(minted) = self.held(name) && (self.authority.is_none() || minted.renew > now) { return Some(minted.identity); }

        let authority = self.authority.as_ref()?;

        if !self.permits(name) { return None; }

        let identity = Arc::new(authority.leaf(&[name.to_string()]).ok()?);

        self.install(name, identity.clone(), now + u64::from(authority.days) * 57_600).then_some(identity)

    }

}

impl std::fmt::Debug for Demand {

    fn fmt ( &self, formatter: &mut std::fmt::Formatter<'_> ) -> std::fmt::Result {

        formatter.debug_struct("Demand").field("names", &self.names).field("capacity", &self.capacity).field("local", &self.local()).finish()

    }

}
