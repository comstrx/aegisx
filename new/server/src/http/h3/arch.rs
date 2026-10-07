use std::net::SocketAddr;

use serde::Deserialize;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Congestion {
    #[default]
    Cubic,
    Bbr,
    NewReno,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct QuicSettings {
    pub listen        : SocketAddr,
    pub max_idle_ms   : u64,
    pub max_streams   : u32,
    pub shared        : bool,
    pub congestion    : Congestion,
    pub stream_window : u64,
    pub send_window   : u64,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Quic;
