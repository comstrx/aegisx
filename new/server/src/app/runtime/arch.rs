use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use crate::app::{Decisions, Errors, Memory, Names, Pools, Resolved, RouteState, Store, Table};
use crate::config::{BackendConfig, Config};
use crate::core::net::Nets;
use crate::core::sync::Swap;
use http::header::HeaderValue;

use crate::http::encode::Compression;
use crate::http::files::FileCache;
use crate::http::variable::Catalog;
use crate::http::tls::{Acceptor, Challenges, Held};

pub struct Snapshot {
    pub version     : u64,
    pub quota       : usize,
    pub actors      : bool,
    pub config      : Arc<Config>,
    pub routes      : Vec<Arc<RouteState>>,
    pub router      : Table,
    pub catalog     : Catalog,
    pub names       : Names,
    pub compression : Compression,
    pub alt_svc     : Option<HeaderValue>,
    pub errors      : Errors,
    pub pages       : bool,
    pub internal    : bool,
    pub paced       : bool,
    pub trusted     : Nets,
}

pub type Tokens = Arc<std::sync::RwLock<std::collections::HashMap<String, String>>>;

pub struct Runtime {
    pub snapshot : Arc<Snapshot>,
    pub pools    : Arc<Pools>,
}

#[derive(Clone, Debug, Default)]
pub struct Overlay {
    pub(super) joined : BTreeMap<( String, String ), BackendConfig>,
    pub(super) left   : BTreeSet<( String, String )>,
}

#[derive(Clone)]
pub struct State {
    pub(super) swap      : Swap<Runtime>,
    pub(super) tls       : Swap<Option<Acceptor>>,
    pub(super) names     : Arc<std::sync::Mutex<Resolved>>,
    pub(super) challenges: Option<Challenges>,
    pub(super) tokens    : Option<Tokens>,
    pub(super) held      : Held,
    pub(super) memory    : Arc<Memory>,
    pub(super) decisions : Option<Arc<Decisions>>,
    pub(super) files     : Arc<FileCache>,
    pub(super) cache     : Option<Arc<Store>>,
    pub(super) base      : Arc<std::sync::Mutex<Config>>,
    pub(super) overlay   : Arc<std::sync::Mutex<Overlay>>,
}
