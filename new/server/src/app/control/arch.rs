use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use bytes::Bytes;

use crate::app::{Analyser, State, Telemetry};
use crate::config::ControlConfig;
use crate::core::secret::Secret;
use crate::core::sys::Resources;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Control;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Grant {
    Admin,
    Backend,
}

pub struct Asset {
    pub bytes : Bytes,
    pub mime  : &'static str,
}

pub struct Panel {
    pub(super) files : HashMap<String, Asset>,
}

pub struct Admin {
    pub(super) state     : State,
    pub(super) telemetry : Arc<Telemetry>,
    pub(super) analyser  : Option<Arc<Analyser>>,
    pub(super) settings  : ControlConfig,
    pub(super) admin     : Secret,
    pub(super) backend   : Option<Secret>,
    pub(super) panel     : Option<Panel>,
    pub(super) started   : Instant,
    pub(super) boot_ms   : u64,
    pub(super) resources : Mutex<Option<Resources>>,
}
