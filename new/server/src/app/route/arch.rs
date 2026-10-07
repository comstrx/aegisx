use std::sync::Arc;

use crate::app::{Basic, Bearer, HashKey, Link};
use crate::config::Route;
use bytes::Bytes;
use http::StatusCode;
use http::header::{HeaderName, HeaderValue};

use crate::core::net::Nets;

use crate::http::files::{Candidate, Files};
use crate::http::proxy::Plan;
use crate::http::encode::Swaps;
use crate::http::fastcgi::Script;
use crate::http::rewrite::Rewrite;
use crate::http::variable::Check;
use crate::http::route::Router;

pub type Table = Router<Arc<RouteState>>;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Policy {
    pub capture    : bool,
    pub rate_limit : u32,
    pub scoped     : bool,
    pub pace       : u32,
    pub burst      : u32,
    pub paced      : bool,
    pub decisions  : bool,
    pub concurrency: u32,
    pub tagged     : bool,
}

#[derive(Clone, Debug)]
pub struct Rule {
    pub key   : HashKey,
    pub scope : u32,
    pub rate  : u32,
    pub burst : u32,
}

#[derive(Debug)]
pub struct RouteState {
    pub index    : usize,
    pub name     : Arc<str>,
    pub pool     : Option<usize>,
    pub mirror   : Option<usize>,
    pub spec     : Route,
    pub plan     : Plan,
    pub policy   : Policy,
    pub keyed    : Option<HashKey>,
    pub rules    : Vec<Rule>,
    pub method   : Option<http::Method>,
    pub bypass   : Vec<usize>,
    pub checks   : Vec<Check>,
    pub replace  : Option<Swaps>,
    pub script   : Option<Script>,
    pub rewrites : Vec<Rewrite>,
    pub files    : Option<Files>,
    pub tries    : Vec<Candidate>,
    pub errors   : Option<Errors>,
    pub auth     : Option<Basic>,
    pub link     : Option<Link>,
    pub bearer   : Option<Arc<Bearer>>,
    pub verify   : Option<Verifier>,
    pub fence    : Option<Fence>,
    pub reply    : Option<Reply>,
}

#[derive(Debug, Default)]
pub struct Fence {
    pub(super) allow : Nets,
    pub(super) deny  : Nets,
}

#[derive(Debug)]
pub struct Reply {
    pub status        : StatusCode,
    pub(super) kind   : HeaderValue,
    pub(super) length : HeaderValue,
    pub(super) body   : Bytes,
}

#[derive(Debug)]
pub struct Verifier {
    pub pool : usize,
    pub path : Box<str>,
    pub copy : Vec<HeaderName>,
    pub plan : Plan,
}

#[derive(Debug, Default)]
pub struct Errors {
    pub(super) pages : Vec<Page>,
}

#[derive(Debug)]
pub struct Page {
    pub(super) statuses : Vec<u16>,
    pub redirect        : Option<HeaderValue>,
    pub path            : Box<str>,
    pub code            : Option<u16>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Routes;
