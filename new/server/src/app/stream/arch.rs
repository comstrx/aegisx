use std::sync::Arc;

use std::cell::Cell;
use std::rc::Rc;

use crate::app::{Fence, Picker, State};
use crate::config::StreamConfig;
use crate::core::sync::Local;

pub struct Relay {
    pub(super) settings : Arc<StreamConfig>,
    pub(super) state    : State,
    pub(super) picker   : Local<Picker>,
    pub(super) pool     : usize,
    pub(super) names    : Vec<( Box<str>, usize )>,
    pub(super) fence    : Option<Fence>,
    pub(super) quota    : usize,
    pub(super) active   : Rc<Cell<usize>>,
}

pub const HELLO_MS: u64 = 5_000;
