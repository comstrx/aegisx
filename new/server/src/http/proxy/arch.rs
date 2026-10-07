use http::header::{HeaderName, HeaderValue};

use crate::http::header::Rendered;
use crate::http::upstream::Upstream;

pub type Extras <'a> = [Option<( &'a HeaderName, &'a HeaderValue )>; 3];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Edit {
    pub from : Box<str>,
    pub to   : Box<str>,
}

#[derive(Debug, Clone, Default)]
pub struct Plan {
    pub timeout_ms       : u64,
    pub max_body_bytes   : usize,
    pub preserve_host    : bool,
    pub underscores      : bool,
    pub mount            : Box<str>,
    pub client_timeout_ms: u64,
    pub buffer_request   : bool,
    pub buffer_response  : usize,
    pub compress         : bool,
    pub cache            : bool,
    pub bandwidth        : u64,
    pub bandwidth_after  : u64,
    pub gunzip           : bool,
    pub request_headers  : Vec<( HeaderName, Rendered )>,
    pub response_headers : Vec<( HeaderName, Rendered )>,
    pub redirects        : Option<Vec<Edit>>,
    pub cookie_domain    : Vec<Edit>,
    pub cookie_path      : Vec<Edit>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Target <'a> {
    Request,
    Canonical(&'a str),
    Rewritten(&'a str),
}

impl Plan {

    pub fn dynamic_request ( &self ) -> bool {

        self.request_headers.iter().any(|( _, value )| matches!(value, Rendered::Dynamic(_)))

    }

    pub fn dynamic_response ( &self ) -> bool {

        self.response_headers.iter().any(|( _, value )| matches!(value, Rendered::Dynamic(_)))

    }

    pub fn edits ( &self ) -> bool {

        self.redirects.is_some() || !self.cookie_domain.is_empty() || !self.cookie_path.is_empty()

    }

}

#[derive(Clone, Copy)]
pub struct Forward <'a> {
    pub plan     : &'a Plan,
    pub upstream : &'a Upstream,
    pub target   : Target<'a>,
    pub add      : Extras<'a>,
    pub rendered : &'a [( HeaderName, HeaderValue )],
    pub drop     : &'a [HeaderName],
    pub upgrade  : Option<&'a HeaderValue>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Proxy;
