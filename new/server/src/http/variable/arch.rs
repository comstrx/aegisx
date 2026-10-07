use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;
use std::sync::Arc;

use bytes::Bytes;
use http::header::HeaderName;
use ipnet::IpNet;
use regex::bytes::Regex;
use serde::Deserialize;

use crate::http::key::HashKey;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    #[default]
    Map,
    Geo,
    Split,
    Mmdb,
    Keyval,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Recipe {
    pub kind    : Kind,
    pub from    : String,
    pub values  : BTreeMap<String, String>,
    pub buckets : BTreeMap<String, u32>,
    pub default : String,
    pub path    : Option<PathBuf>,
    pub field   : String,
}

#[derive(Clone, Debug)]
pub enum Step {
    Key(Box<str>),
    Index(usize),
}

#[derive(Deserialize)]
#[serde(untagged)]
pub(super) enum Scalar <'a> {
    Text(&'a str),
    Unsigned(u64),
    Signed(i64),
    Float(f64),
    Flag(bool),
}

#[derive(Clone, Debug)]
pub enum Source {
    Map { key: HashKey, exact: HashMap<Box<[u8]>, Bytes>, patterns: Vec<( Regex, Bytes )> },
    Geo { nets: Vec<( IpNet, Bytes )> },
    Split { key: HashKey, buckets: Vec<( u64, Bytes )>, total: u64 },
    Mmdb { reader: Arc<maxminddb::Reader<Vec<u8>>>, steps: Box<[Step]> },
    Keyval { key: HashKey, zone: Zone },
}

pub type Zone = Arc<std::sync::RwLock<HashMap<Box<[u8]>, Bytes>>>;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Keyval;

#[derive(Clone, Debug)]
pub struct Derived {
    pub name            : Box<str>,
    pub(super) source   : Source,
    pub(super) fallback : Bytes,
}

#[derive(Clone, Debug, Default)]
pub struct Catalog {
    pub(super) list : Vec<Derived>,
    pub located     : bool,
}

#[derive(Clone, Debug)]
pub enum Subject {
    Header(HeaderName),
    Query(Box<str>),
    Derived(usize),
}

#[derive(Clone, Debug)]
pub enum Test {
    Exact(Box<[u8]>),
    Pattern(Regex),
    Present,
}

#[derive(Clone, Debug)]
pub struct Check {
    pub(super) subject : Subject,
    pub(super) test    : Test,
    pub(super) negate  : bool,
}
