use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock, RwLock};

use memchr::memmem::Finder;
use ort::session::Session;
use serde::{Deserialize, Serialize};

pub const INPUT_SCHEMA: &str = "byte-journey-v1";
pub const ADMISSION: usize = 16;
pub const CONTENT_FIXED: usize = 16;
pub const OUTCOME: usize = 8;
pub const PATTERN_GROUPS: usize = 6;

pub struct Schema {
    pub version  : u32,
    pub names    : Vec<String>,
    pub scales   : Vec<f32>,
    pub patterns : Vec<Vec<Finder<'static>>>,
    pub buckets  : usize,
}

pub static REGISTRY: OnceLock<RwLock<HashMap<Arc<str>, Arc<Model>>>> = OnceLock::new();

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Models;

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct Compatible {
    pub model_version   : String,
    pub parameter_count : usize,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct Architecture {
    pub name                     : String,
    pub text_bytes               : usize,
    pub text_streams             : usize,
    pub event_count              : usize,
    pub event_bytes              : usize,
    pub event_values             : usize,
    pub coverage                 : usize,
    pub feature_count            : usize,
    pub parameter_count          : usize,
    #[serde(default)]
    pub compatible_architectures : Vec<Compatible>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct Meta {
    pub model_version     : String,
    #[serde(default)]
    pub input_schema      : Option<String>,
    pub feature_version   : u32,
    pub parameter_count   : usize,
    #[serde(default = "fp32")]
    pub precision         : String,
    pub artifact_sha256   : String,
    pub source            : String,
    #[serde(default)]
    pub deployment_ready  : bool,
    #[serde(default)]
    pub evaluation_notice : Option<String>,
    pub architecture      : Architecture,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Input {
    pub features     : Vec<f32>,
    pub text         : Vec<i64>,
    pub event_text   : Vec<i64>,
    pub event_values : Vec<f32>,
    pub coverage     : Vec<f32>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize)]
pub struct Scores {
    pub risk    : f32,
    pub content : f32,
    pub journey : f32,
}

pub struct Assist {
    pub(super) model     : Arc<Model>,
    pub(super) input     : Input,
    pub(super) threshold : f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct Assessment {
    pub scores    : Scores,
    pub risky     : bool,
    pub dominant  : &'static str,
    pub threshold : f32,
}

pub struct Model {
    pub(super) name    : Arc<str>,
    pub(super) meta    : Meta,
    pub(super) schema  : Schema,
    pub(super) session : Mutex<Session>,
    pub(super) sha256  : String,
}

fn fp32 () -> String {

    "fp32".to_string()

}
