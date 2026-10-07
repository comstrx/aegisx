use std::collections::HashMap;

use foldhash::fast::RandomState;
use http::header::HeaderValue;

use crate::core::cache::Cache;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Auth;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Credential {
    Bcrypt(Box<str>),
    Sha1([u8; 20]),
    Plain(Box<str>),
}

pub struct Basic {
    pub(super) realm  : HeaderValue,
    pub(super) users  : HashMap<Box<str>, Credential, RandomState>,
    pub(super) recent : Cache<[u8; 32], ()>,
}

impl std::fmt::Debug for Basic {

    fn fmt ( &self, formatter: &mut std::fmt::Formatter<'_> ) -> std::fmt::Result {

        formatter.debug_struct("Basic").field("realm", &self.realm).field("users", &self.users.len()).finish()

    }

}

pub type Grant = std::sync::Arc<[( http::header::HeaderName, HeaderValue )]>;

pub struct Bearer {
    pub(super) verifier : crate::core::jwt::Verifier,
    pub(super) header   : http::header::HeaderName,
    pub(super) scheme   : bool,
    pub(super) cookie   : Option<Box<str>>,
    pub(super) claims   : Vec<( Box<str>, http::header::HeaderName )>,
    pub(super) recent   : Cache<Box<[u8]>, Grant>,
}

impl std::fmt::Debug for Bearer {

    fn fmt ( &self, formatter: &mut std::fmt::Formatter<'_> ) -> std::fmt::Result {

        formatter.debug_struct("Bearer").field("header", &self.header).field("claims", &self.claims.len()).finish()

    }

}

pub struct Link {
    pub(super) key       : aws_lc_rs::hmac::Key,
    pub(super) signature : Box<str>,
    pub(super) expires   : Box<str>,
}

impl std::fmt::Debug for Link {

    fn fmt ( &self, formatter: &mut std::fmt::Formatter<'_> ) -> std::fmt::Result {

        formatter.debug_struct("Link").field("signature", &self.signature).field("expires", &self.expires).finish()

    }

}
