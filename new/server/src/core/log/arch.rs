use std::sync::OnceLock;

pub static READY: OnceLock<bool> = OnceLock::new();

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Log;
