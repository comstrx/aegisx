use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

pub static SLOTS: OnceLock<Mutex<HashMap<Box<str>, usize>>> = OnceLock::new();

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Slot;
