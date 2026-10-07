use std::sync::OnceLock;
use std::thread::JoinHandle;

use tokio::runtime::Runtime;

use crate::core::error::AppResult;

pub static MAIN: OnceLock<Runtime> = OnceLock::new();

pub static RELOAD: tokio::sync::Notify = tokio::sync::Notify::const_new();

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Rt;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Event {
    Reload,
    Terminate,
}

pub struct Workers {
    pub(super) handles: Vec<JoinHandle<AppResult<()>>>,
}
