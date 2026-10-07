use std::collections::HashSet;
use std::sync::{Mutex, OnceLock};

pub static INTERNED: OnceLock<Mutex<HashSet<&'static str>>> = OnceLock::new();

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Str;
