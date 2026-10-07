use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;

use arc_swap::ArcSwap;
use tokio::sync::watch;

pub struct Local <T> {
    pub(super) inner: Rc<RefCell<T>>,
}

pub struct Swap <T> {
    pub(super) inner : Arc<ArcSwap<T>>,
    pub(super) epoch : Arc<AtomicU64>,
}

pub struct View <T> {
    pub(super) inner : Arc<T>,
}

pub struct Lens <T> {
    pub(super) source : Swap<T>,
    pub(super) epoch  : Cell<u64>,
    pub(super) held   : RefCell<Rc<View<T>>>,
}

#[derive(Clone)]
pub struct Signal {
    pub(super) sender: watch::Sender<bool>,
}

#[derive(Clone)]
pub struct Watch {
    pub(super) receiver: watch::Receiver<bool>,
}

pub struct Shared <T> {
    pub(super) inner: Arc<std::sync::Mutex<T>>,
}

#[derive(Default)]
pub struct Tally {
    pub(super) value: AtomicU64,
}
