use aws_lc_rs::{hmac, signature};
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use serde::Deserialize;
use serde_json::Value;

use crate::core::error::{AppError, AppResult};
use super::arch::{Algorithm, Claims, Fault, Key, Material, Verifier};

#[derive(Deserialize)]
struct Head {
    alg : String,
    #[serde(default)]
    kid : Option<String>,
}

#[derive(Deserialize)]
struct Set {
    keys : Vec<Jwk>,
}

#[derive(Deserialize)]
struct Jwk {
    kty : String,
    #[serde(default)]
    kid : Option<String>,
    #[serde(default)]
    crv : Option<String>,
    #[serde(default)]
    n   : Option<String>,
    #[serde(default)]
    e   : Option<String>,
    #[serde(default)]
    x   : Option<String>,
    #[serde(default)]
    y   : Option<String>,
}

impl Algorithm {

    pub fn named ( name: &str ) -> Option<Self> {

        Some(match name {
            "HS256" => Self::Hs256,
            "HS384" => Self::Hs384,
            "HS512" => Self::Hs512,
            "RS256" => Self::Rs256,
            "RS384" => Self::Rs384,
            "RS512" => Self::Rs512,
            "PS256" => Self::Ps256,
            "PS384" => Self::Ps384,
            "PS512" => Self::Ps512,
            "ES256" => Self::Es256,
            "ES384" => Self::Es384,
            "EdDSA" => Self::EdDsa,
            _ => return None,
        })

    }

    pub fn symmetric ( self ) -> bool {

        matches!(self, Self::Hs256 | Self::Hs384 | Self::Hs512)

    }

    fn check ( self, material: &Material, signed: &[u8], tag: &[u8] ) -> bool {

        let rsa = |parameters: &'static signature::RsaParameters, modulus: &[u8], exponent: &[u8]| signature::RsaPublicKeyComponents { n: modulus, e: exponent }.verify(parameters, signed, tag).is_ok();
        let curve = |algorithm: &'static dyn signature::VerificationAlgorithm, point: &[u8]| signature::UnparsedPublicKey::new(algorithm, point).verify(signed, tag).is_ok();
        let mac = |algorithm: hmac::Algorithm, secret: &[u8]| hmac::verify(&hmac::Key::new(algorithm, secret), signed, tag).is_ok();

        match ( self, material ) {
            ( Self::Hs256, Material::Secret(secret) ) => mac(hmac::HMAC_SHA256, secret),
            ( Self::Hs384, Material::Secret(secret) ) => mac(hmac::HMAC_SHA384, secret),
            ( Self::Hs512, Material::Secret(secret) ) => mac(hmac::HMAC_SHA512, secret),
            ( Self::Rs256, Material::Rsa { modulus, exponent } ) => rsa(&signature::RSA_PKCS1_2048_8192_SHA256, modulus, exponent),
            ( Self::Rs384, Material::Rsa { modulus, exponent } ) => rsa(&signature::RSA_PKCS1_2048_8192_SHA384, modulus, exponent),
            ( Self::Rs512, Material::Rsa { modulus, exponent } ) => rsa(&signature::RSA_PKCS1_2048_8192_SHA512, modulus, exponent),
            ( Self::Ps256, Material::Rsa { modulus, exponent } ) => rsa(&signature::RSA_PSS_2048_8192_SHA256, modulus, exponent),
            ( Self::Ps384, Material::Rsa { modulus, exponent } ) => rsa(&signature::RSA_PSS_2048_8192_SHA384, modulus, exponent),
            ( Self::Ps512, Material::Rsa { modulus, exponent } ) => rsa(&signature::RSA_PSS_2048_8192_SHA512, modulus, exponent),
            ( Self::Es256, Material::P256(point) ) => curve(&signature::ECDSA_P256_SHA256_FIXED, point),
            ( Self::Es384, Material::P384(point) ) => curve(&signature::ECDSA_P384_SHA384_FIXED, point),
            ( Self::EdDsa, Material::Ed25519(point) ) => curve(&signature::ED25519, point),
            _ => false,
        }

    }

}

impl Verifier {

    pub fn new ( keys: Vec<Key>, allowed: Vec<Algorithm>, issuer: Option<&str>, audience: Option<&str>, leeway: u64 ) -> AppResult<Self> {

        if keys.is_empty() { return Err(AppError::config("jwt", "needs a secret or a key set")); }

        let secret = keys.iter().any(|key| matches!(key.material, Material::Secret(_)));

        if secret && keys.iter().any(|key| !matches!(key.material, Material::Secret(_))) { return Err(AppError::config("jwt", "a shared secret and public keys cannot serve the same verifier")); }

        let allowed = match allowed.is_empty() {
            true if secret => vec![Algorithm::Hs256, Algorithm::Hs384, Algorithm::Hs512],
            true => vec![Algorithm::Rs256, Algorithm::Rs384, Algorithm::Rs512, Algorithm::Ps256, Algorithm::Ps384, Algorithm::Ps512, Algorithm::Es256, Algorithm::Es384, Algorithm::EdDsa],
            false => allowed,
        };

        if allowed.iter().any(|algorithm| algorithm.symmetric() != secret) { return Err(AppError::config("jwt", "algorithms do not fit the configured keys")); }

        Ok(Self { keys, allowed, issuer: issuer.map(Into::into), audience: audience.map(Into::into), leeway })

    }

    pub fn secret ( secret: &[u8] ) -> Key {

        Key { id: None, material: Material::Secret(secret.into()) }

    }

    pub fn jwks ( json: &[u8] ) -> AppResult<Vec<Key>> {

        let set: Set = serde_json::from_slice(json).map_err(|error| AppError::parse("jwks", error.to_string()))?;
        let part = |value: &Option<String>| value.as_deref().and_then(|text| URL_SAFE_NO_PAD.decode(text.trim_end_matches('=')).ok());
        let point = |jwk: &Jwk| { let ( x, y ) = ( part(&jwk.x)?, part(&jwk.y)? ); let mut bytes = Vec::with_capacity(1 + x.len() + y.len()); bytes.push(4); bytes.extend_from_slice(&x); bytes.extend_from_slice(&y); Some(bytes.into_boxed_slice()) };

        let keys: Vec<Key> = set.keys.iter().filter_map(|jwk| {

            let material = match ( jwk.kty.as_str(), jwk.crv.as_deref() ) {
                ( "RSA", _ ) => Material::Rsa { modulus: part(&jwk.n)?.into(), exponent: part(&jwk.e)?.into() },
                ( "EC", Some("P-256") ) => Material::P256(point(jwk)?),
                ( "EC", Some("P-384") ) => Material::P384(point(jwk)?),
                ( "OKP", Some("Ed25519") ) => Material::Ed25519(part(&jwk.x)?.into()),
                _ => return None,
            };

            Some(Key { id: jwk.kid.as_deref().map(Into::into), material })

        }).collect();

        if keys.is_empty() { return Err(AppError::parse("jwks", "no usable RSA, P-256, P-384 or Ed25519 key")); }

        Ok(keys)

    }

    pub fn verify ( &self, token: &[u8], now: u64 ) -> Result<Claims, Fault> {

        let mut parts = token.split(|byte| *byte == b'.');
        let ( Some(head), Some(body), Some(tag), None ) = ( parts.next(), parts.next(), parts.next(), parts.next() ) else { return Err(Fault::Malformed); };
        let decode = |part: &[u8]| URL_SAFE_NO_PAD.decode(part).map_err(|_| Fault::Malformed);
        let header: Head = serde_json::from_slice(&decode(head)?).map_err(|_| Fault::Malformed)?;
        let algorithm = Algorithm::named(&header.alg).filter(|algorithm| self.allowed.contains(algorithm)).ok_or(Fault::Algorithm)?;
        let signed = &token[..head.len() + 1 + body.len()];
        let tag = decode(tag)?;
        let fits = |key: &&Key| match ( header.kid.as_deref(), key.id.as_deref() ) { ( Some(wanted), Some(id) ) => wanted == id, _ => true };

        if !self.keys.iter().filter(fits).any(|key| algorithm.check(&key.material, signed, &tag)) { return Err(Fault::Signature); }

        let claims: Claims = serde_json::from_slice(&decode(body)?).map_err(|_| Fault::Malformed)?;
        let moment = |name: &str| claims.get(name).and_then(Value::as_u64);

        if moment("exp").is_some_and(|expires| now >= expires.saturating_add(self.leeway)) { return Err(Fault::Expired); }

        if moment("nbf").is_some_and(|starts| now.saturating_add(self.leeway) < starts) { return Err(Fault::Early); }

        if let Some(issuer) = &self.issuer && claims.get("iss").and_then(Value::as_str) != Some(issuer) { return Err(Fault::Issuer); }

        if let Some(audience) = &self.audience {

            let named = match claims.get("aud") {
                Some(Value::String(one)) => **audience == **one,
                Some(Value::Array(many)) => many.iter().any(|one| one.as_str() == Some(audience)),
                _ => false,
            };

            if !named { return Err(Fault::Audience); }

        }

        Ok(claims)

    }

}

impl Fault {

    pub fn reason ( self ) -> &'static str {

        match self {
            Self::Missing => "missing token",
            Self::Malformed => "malformed token",
            Self::Algorithm => "algorithm not allowed",
            Self::Signature => "signature mismatch",
            Self::Expired => "token expired",
            Self::Early => "token not valid yet",
            Self::Issuer => "issuer mismatch",
            Self::Audience => "audience mismatch",
        }

    }

}
