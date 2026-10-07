use std::sync::{Arc, Mutex};

use super::arch::Shared;

impl <T> Shared <T> {

    pub fn new ( value: T ) -> Self {

        Self { inner: Arc::new(Mutex::new(value)) }

    }

    pub fn with <R> ( &self, func: impl FnOnce(&T) -> R ) -> R {

        func(&self.inner.lock().unwrap_or_else(|poisoned| poisoned.into_inner()))

    }

    pub fn with_mut <R> ( &self, func: impl FnOnce(&mut T) -> R ) -> R {

        func(&mut self.inner.lock().unwrap_or_else(|poisoned| poisoned.into_inner()))

    }

}

impl <T> Clone for Shared <T> {

    fn clone ( &self ) -> Self {

        Self { inner: Arc::clone(&self.inner) }

    }

}
