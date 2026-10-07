use std::sync::{Arc, Mutex};

use crate::app::{Access, Analyser, State, Telemetry};
use crate::config::Config;
use crate::core::rt::Workers;
use crate::core::sync::{Signal, Watch};
use crate::http::server::Listener;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Boot;

pub struct Running {
    pub(super) config    : Arc<Config>,
    pub(super) state     : State,
    pub(super) telemetry : Arc<Telemetry>,
    pub(super) analyser  : Option<Arc<Analyser>>,
    pub(super) access    : Option<Arc<Access>>,
    pub(super) signal    : Signal,
    pub(super) workers   : Workers,
    pub(super) control   : Option<Workers>,
}

pub struct Worker {
    pub(super) index     : usize,
    pub(super) config    : Arc<Config>,
    pub(super) state     : State,
    pub(super) telemetry : Arc<Telemetry>,
    pub(super) analyser  : Option<Arc<Analyser>>,
    pub(super) access    : Option<Arc<Access>>,
    pub(super) listeners : Arc<Mutex<Vec<Option<Listener>>>>,
    pub(super) streams   : Arc<Mutex<Vec<Vec<Option<Listener>>>>>,
    pub(super) datagrams : Arc<Mutex<Vec<Vec<Option<std::net::UdpSocket>>>>>,
    pub(super) extras    : Arc<Mutex<Vec<Vec<Option<Listener>>>>>,
    pub(super) quic      : Arc<Mutex<Vec<Option<std::net::UdpSocket>>>>,
    pub(super) stop      : Watch,
}
