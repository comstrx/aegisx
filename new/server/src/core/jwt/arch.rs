#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Algorithm {
    Hs256,
    Hs384,
    Hs512,
    Rs256,
    Rs384,
    Rs512,
    Ps256,
    Ps384,
    Ps512,
    Es256,
    Es384,
    EdDsa,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Material {
    Secret(Box<[u8]>),
    Rsa { modulus: Box<[u8]>, exponent: Box<[u8]> },
    P256(Box<[u8]>),
    P384(Box<[u8]>),
    Ed25519(Box<[u8]>),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Key {
    pub id       : Option<Box<str>>,
    pub material : Material,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Fault {
    Missing,
    Malformed,
    Algorithm,
    Signature,
    Expired,
    Early,
    Issuer,
    Audience,
}

pub type Claims = serde_json::Map<String, serde_json::Value>;

#[derive(Clone, Debug)]
pub struct Verifier {
    pub(super) keys     : Vec<Key>,
    pub(super) allowed  : Vec<Algorithm>,
    pub(super) issuer   : Option<Box<str>>,
    pub(super) audience : Option<Box<str>>,
    pub(super) leeway   : u64,
}
