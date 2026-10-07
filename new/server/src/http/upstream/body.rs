use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::Duration;

use bytes::Bytes;
use http_body::{Body as _, Frame, SizeHint};
use hyper::upgrade::OnUpgrade;
use send_wrapper::SendWrapper;
use tokio::time::{Instant, sleep};

use crate::core::error::AppError;
use crate::http::body::{Body, Guard, Incoming, Probe};
use super::arch::{Outbound, Streaming};

impl Outbound {

    pub fn new ( body: Body ) -> Self {

        Self { body: SendWrapper::new(body) }

    }

}

impl http_body::Body for Outbound {

    type Data = Bytes;
    type Error = AppError;

    fn poll_frame ( self: Pin<&mut Self>, cx: &mut Context<'_> ) -> Poll<Option<Result<Frame<Bytes>, AppError>>> {

        Pin::new(&mut *self.get_mut().body).poll_frame(cx)

    }

    fn is_end_stream ( &self ) -> bool {

        self.body.is_end_stream()

    }

    fn size_hint ( &self ) -> SizeHint {

        self.body.size_hint()

    }

}

impl Streaming {

    pub(super) fn new ( incoming: Incoming, idle_ms: u64 ) -> Self {

        Self { first: None, incoming, idle_ms, timer: None, armed: false, upgrade: None, guard: None, probe: None }

    }

    pub(super) fn prefixed ( mut self, first: Option<Frame<Bytes>> ) -> Self {

        self.first = first;

        self

    }

    pub(super) fn upgradable ( mut self, upgrade: Option<OnUpgrade> ) -> Self {

        self.upgrade = upgrade;

        self

    }

    pub fn guard ( &mut self, guard: Guard ) {

        self.guard = Some(guard);

    }

    pub fn probe ( &mut self, probe: Probe ) {

        self.probe = Some(probe);

    }

    pub fn is_end ( &self ) -> bool {

        self.first.is_none() && self.incoming.is_end_stream()

    }

    pub fn hint ( &self ) -> SizeHint {

        let inner = self.incoming.size_hint();
        let first = self.first.as_ref().and_then(Frame::data_ref).map_or(0, |data| data.len() as u64);
        let mut hint = SizeHint::new();

        if let Some(upper) = inner.upper() { hint.set_upper(upper + first); }

        hint.set_lower(inner.lower() + first);

        hint

    }

    pub(super) fn detach ( mut self ) -> Option<OnUpgrade> {

        self.upgrade.take()

    }

    fn finish ( &mut self ) {

        if let Some(probe) = &self.probe { probe.borrow_mut().finish(); }

    }

    fn stalled ( &mut self, cx: &mut Context<'_> ) -> bool {

        if self.idle_ms == 0 { return false; }

        let idle = Duration::from_millis(self.idle_ms);
        let timer = self.timer.get_or_insert_with(|| Box::pin(sleep(idle)));

        if !self.armed { timer.as_mut().reset(Instant::now() + idle); self.armed = true; }

        timer.as_mut().poll(cx).is_ready()

    }

}

impl http_body::Body for Streaming {

    type Data = Bytes;
    type Error = AppError;

    fn poll_frame ( self: Pin<&mut Self>, cx: &mut Context<'_> ) -> Poll<Option<Result<Frame<Bytes>, AppError>>> {

        let this = self.get_mut();

        if let Some(frame) = this.first.take() {

            if let Some(data) = frame.data_ref() && let Some(probe) = &this.probe { probe.borrow_mut().feed(data); }

            if this.incoming.is_end_stream() { this.finish(); }

            return Poll::Ready(Some(Ok(frame)));

        }

        match Pin::new(&mut this.incoming).poll_frame(cx) {
            Poll::Ready(Some(Ok(frame))) => {

                this.armed = false;

                if let Some(data) = frame.data_ref() && let Some(probe) = &this.probe { probe.borrow_mut().feed(data); }

                if this.incoming.is_end_stream() { this.finish(); }

                Poll::Ready(Some(Ok(frame)))

            }
            Poll::Ready(Some(Err(error))) => Poll::Ready(Some(Err(AppError::network("upstream body", error.to_string())))),
            Poll::Ready(None) => { this.finish(); Poll::Ready(None) }
            Poll::Pending => if this.stalled(cx) { Poll::Ready(Some(Err(AppError::timeout("upstream body", this.idle_ms)))) } else { Poll::Pending },
        }

    }

    fn is_end_stream ( &self ) -> bool {

        self.is_end()

    }

    fn size_hint ( &self ) -> SizeHint {

        self.hint()

    }

}
