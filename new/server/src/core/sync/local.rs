use std::cell::RefCell;
use std::rc::Rc;

use super::arch::Local;

impl <T> Local <T> {

    pub fn new ( value: T ) -> Self {

        Self { inner: Rc::new(RefCell::new(value)) }

    }

    pub fn with <R> ( &self, func: impl FnOnce(&T) -> R ) -> R {

        func(&self.inner.borrow())

    }

    pub fn with_mut <R> ( &self, func: impl FnOnce(&mut T) -> R ) -> R {

        func(&mut self.inner.borrow_mut())

    }

    pub fn replace ( &self, value: T ) -> T {

        self.inner.replace(value)

    }

}

impl <T> Clone for Local <T> {

    fn clone ( &self ) -> Self {

        Self { inner: Rc::clone(&self.inner) }

    }

}
