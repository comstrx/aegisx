use std::{net::IpAddr, sync::Arc, time::Instant};
use crate::core::capacity::Permit;

use crate::core::domain::Actor;
use crate::module::{cache::{Fill, Key}, decision::Inspection, runtime::{RouteState, Snapshot},
    services::Services, telemetry::Ticket, upstream::Lease};

pub struct Proxy { pub(super) services: Arc<Services> }

pub struct Context {
    pub(super) id: String,
    pub(super) id_value: Option<http::HeaderValue>,
    pub(super) started: Instant,
    pub(super) snapshot: Arc<Snapshot>,
    pub(super) route: Option<Arc<RouteState>>,
    pub(super) canonical: Option<String>,
    pub(super) lease: Option<Lease>,
    pub(super) excluded: [usize;3],
    pub(super) attempts: usize,
    pub(super) upstream_started: Instant,
    pub(super) permit: Option<Permit>,
    pub(super) wait_deadline: Option<tokio::time::Instant>,
    pub(super) waited: bool,
    pub(super) peer: Option<IpAddr>,
    pub(super) actor: Option<Actor>,
    pub(super) inspection: Inspection,
    pub(super) ticket: Option<Ticket>,
    pub(super) sequence: u32,
    pub(super) blocked: bool,
    pub(super) model_rejected: bool,
    pub(super) capture: bool,
    pub(super) request_bytes: usize,
    pub(super) sample: Vec<u8>,
    pub(super) sample_seen: usize,
    pub(super) response_sample: Vec<u8>,
    pub(super) response_seen: usize,
    pub(super) response_available: bool,
    pub(super) forwarded: bool,
    pub(super) generation: u64,
    pub(super) storage: Option<crate::module::storage::Reservation>,
    pub(super) analysis_slot: Option<crate::module::inference::Reservation>,
    pub(super) events: Vec<crate::module::storage::Event>,
    pub(super) journey: Option<crate::module::lifecycle::Handle>,
    pub(super) background: Option<[f32; 16]>,
    pub(super) cache_key: Option<Key>,
    pub(super) fill: Option<Fill>,
    pub(super) cache_hit: bool,
}

impl std::ops::Deref for Proxy {
    type Target = Services;
    fn deref ( &self ) -> &Services { &self.services }
}
