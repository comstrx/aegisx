use std::sync::Arc;
use std::sync::atomic::AtomicU64;

use serde::{Deserialize, Serialize};
use tokio::sync::oneshot;

use crate::app::Key;
use crate::core::cache::Cache;
use crate::core::db::Db;
use crate::core::error::AppResult;
use crate::core::queue::Queue;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Verdict {
    pub key        : String,
    pub actor      : String,
    pub route      : String,
    pub reason     : String,
    pub source     : String,
    pub created_ms : u64,
    pub expires_ms : u64,
    pub request_id : Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Block {
    pub config_version : String,
    #[serde(default)]
    pub request_id     : Option<String>,
    pub route          : String,
    pub actor          : String,
    pub ttl_ms         : u64,
    pub reason         : String,
}

pub struct Write {
    pub(super) key     : Key,
    pub(super) verdict : Option<Arc<Verdict>>,
    pub(super) done    : Option<oneshot::Sender<AppResult<()>>>,
}

pub struct Decisions {
    pub(super) db             : Arc<Db>,
    pub(super) cache          : Arc<Cache<Key, Arc<Verdict>>>,
    pub(super) writer         : Queue<Write>,
    pub(super) deny_ttl_ms    : u64,
    pub(super) hits           : AtomicU64,
    pub(super) misses         : AtomicU64,
    pub(super) write_failures : Arc<AtomicU64>,
    pub(super) generation     : AtomicU64,
}
