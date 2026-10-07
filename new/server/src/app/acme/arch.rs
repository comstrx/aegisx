use std::sync::Arc;

use crate::app::{State, Tokens};
use crate::config::{AcmeConfig, OnDemandConfig};
use crate::http::tls::{Challenges, Demand};
use crate::http::upstream::Settings;

pub struct Acme {
    pub(super) settings : AcmeConfig,
    pub(super) state    : State,
}

#[derive(Clone)]
pub struct Summon {
    pub(super) settings   : AcmeConfig,
    pub(super) plan       : OnDemandConfig,
    pub(super) demand     : Arc<Demand>,
    pub(super) challenges : Option<Challenges>,
    pub(super) tokens     : Option<Tokens>,
    pub(super) client     : Settings,
    pub(super) fixed      : Arc<[String]>,
    pub(super) gate       : Arc<tokio::sync::Mutex<()>>,
    pub(super) budget     : Arc<std::sync::Mutex<( u64, u32 )>>,
}
