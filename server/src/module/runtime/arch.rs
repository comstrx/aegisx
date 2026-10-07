use std::collections::BTreeMap;
use std::sync::Arc;
use std::thread::JoinHandle;

use tokio::sync::oneshot;

use crate::module::config::{Config, Limits, Mode, Route};
use crate::module::upstream::Pool;

pub struct Policy {
    pub limits: Limits,
    pub mode: Mode,
    pub threshold: f32,
    pub content_threshold: f32,
    pub journey_threshold: f32,
    pub capture: bool,
    pub preserve_host: bool,
    pub request_headers: Vec<(http::HeaderName, http::HeaderValue)>,
    pub response_headers: Vec<(http::HeaderName, http::HeaderValue)>,
}

pub struct RouteState {
    pub pool: Option<Arc<Pool>>,
    pub decision_namespace: [u8;32],
    pub spec: Route,
    pub name: Arc<str>,
    pub policy: Policy,
}

pub struct Snapshot {
    pub identity_headers: crate::module::identity::Headers,
    pub version: String,
    pub config: Config,
    pub pools: BTreeMap<String, Arc<Pool>>,
    pub routes: Vec<Arc<RouteState>>,
    pub(super) index: super::index::RouteIndex,
    pub fallback: Option<Arc<RouteState>>,
}

pub struct Manager;

pub struct ManagerGuard {
    pub(super) stop: Option<oneshot::Sender<()>>,
    pub(super) thread: Option<JoinHandle<()>>,
}
