use std::cell::Cell;
use std::future::Future;
use std::pin::Pin;
use std::rc::Rc;
use std::task::{Context, Poll, Waker};

use crate::core::sync::Local;
use super::arch::{Graceful, Slot};

impl Slot {

    pub fn claim ( active: &Local<usize> ) -> Self {

        active.with_mut(|count| *count += 1);

        Self { active: active.clone() }

    }

}

impl Drop for Slot {

    fn drop ( &mut self ) {

        self.active.with_mut(|count| *count = count.saturating_sub(1));

    }

}

impl <F> Graceful<F> {

    pub fn new ( inner: F, close: fn(Pin<&mut F>), after: u8, stage: &Rc<Cell<u8>>, waker: &Rc<Cell<Option<Waker>>> ) -> Self {

        Self { inner: Box::pin(inner), close, stage: stage.clone(), waker: waker.clone(), after, armed: false, closed: false }

    }

}

impl <F: Future> Future for Graceful<F> {

    type Output = F::Output;

    fn poll ( mut self: Pin<&mut Self>, context: &mut Context<'_> ) -> Poll<F::Output> {

        if !self.armed {

            self.armed = true;
            self.waker.set(Some(context.waker().clone()));

        }

        if !self.closed && self.stage.get() >= self.after {

            let close = self.close;

            self.closed = true;

            close(self.inner.as_mut());

        }

        self.inner.as_mut().poll(context)

    }

}
