use std::cell::{Cell, RefCell};
use std::ops::Deref;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use arc_swap::ArcSwap;

use super::arch::{Lens, Swap, View};

impl <T> Swap <T> {

    pub fn new ( value: T ) -> Self {

        Self { inner: Arc::new(ArcSwap::from_pointee(value)), epoch: Arc::new(AtomicU64::new(0)) }

    }

    pub fn load ( &self ) -> Arc<T> {

        self.inner.load_full()

    }

    pub fn store ( &self, value: T ) {

        self.inner.store(Arc::new(value));
        self.epoch.fetch_add(1, Ordering::Release);

    }

    pub fn epoch ( &self ) -> u64 {

        self.epoch.load(Ordering::Acquire)

    }

    pub fn with <R> ( &self, func: impl FnOnce(&T) -> R ) -> R {

        func(&self.inner.load())

    }

    pub fn lens ( &self ) -> Lens<T> {

        Lens { source: self.clone(), epoch: Cell::new(self.epoch()), held: RefCell::new(Rc::new(View { inner: self.load() })) }

    }

}

impl <T> Clone for Swap <T> {

    fn clone ( &self ) -> Self {

        Self { inner: Arc::clone(&self.inner), epoch: Arc::clone(&self.epoch) }

    }

}

impl <T> Lens <T> {

    pub fn get ( &self ) -> Rc<View<T>> {

        let epoch = self.source.epoch();

        if epoch != self.epoch.get() {

            *self.held.borrow_mut() = Rc::new(View { inner: self.source.load() });
            self.epoch.set(epoch);

        }

        self.held.borrow().clone()

    }

}

impl <T> Deref for View <T> {

    type Target = T;

    fn deref ( &self ) -> &T {

        &self.inner

    }

}
