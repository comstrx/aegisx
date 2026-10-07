use std::sync::Arc;

use rcgen::{CertificateParams, CustomExtension, DistinguishedName, DnType, KeyPair};
use rustls::crypto::aws_lc_rs;
use rustls::pki_types::{PrivateKeyDer, PrivatePkcs8KeyDer};
use rustls::sign::CertifiedKey;

use crate::core::error::{AppError, AppResult};
use super::arch::Tls;

impl Tls {

    pub fn self_signed ( names: &[String], common_name: &str ) -> AppResult<( String, String )> {

        let key = KeyPair::generate().map_err(|error| AppError::message(format!("cannot generate key: {error}")))?;
        let mut params = CertificateParams::new(names.to_vec()).map_err(|error| AppError::message(format!("invalid certificate names: {error}")))?;

        params.distinguished_name = DistinguishedName::new();
        params.distinguished_name.push(DnType::CommonName, common_name);

        let cert = params.self_signed(&key).map_err(|error| AppError::message(format!("cannot self-sign: {error}")))?;

        Ok(( cert.pem(), key.serialize_pem() ))

    }

    pub fn challenge ( name: &str, digest: &[u8] ) -> AppResult<Arc<CertifiedKey>> {

        let key = KeyPair::generate().map_err(|error| AppError::message(format!("cannot generate key: {error}")))?;
        let mut params = CertificateParams::new(vec![name.to_string()]).map_err(|error| AppError::message(format!("invalid challenge name: {error}")))?;

        params.distinguished_name = DistinguishedName::new();
        params.custom_extensions = vec![CustomExtension::new_acme_identifier(digest)];

        let cert = params.self_signed(&key).map_err(|error| AppError::message(format!("cannot sign challenge certificate: {error}")))?;
        let private = PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(key.serialize_der()));
        let signing = aws_lc_rs::sign::any_supported_type(&private).map_err(|error| AppError::message(format!("challenge key unsupported: {error}")))?;

        Ok(Arc::new(CertifiedKey::new(vec![cert.der().clone()], signing)))

    }

    pub fn csr ( names: &[String] ) -> AppResult<( Vec<u8>, String )> {

        let key = KeyPair::generate().map_err(|error| AppError::message(format!("cannot generate key: {error}")))?;
        let mut params = CertificateParams::new(names.to_vec()).map_err(|error| AppError::message(format!("invalid certificate names: {error}")))?;

        params.distinguished_name = DistinguishedName::new();

        let request = params.serialize_request(&key).map_err(|error| AppError::message(format!("cannot build csr: {error}")))?;

        Ok(( request.der().to_vec(), key.serialize_pem() ))

    }

}
