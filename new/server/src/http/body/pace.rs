use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::{Duration, Instant};

use bytes::Bytes;
use http_body::Body as _;
use http_body::Frame;

use crate::core::error::AppError;
use super::arch::{Body, Guard, Paced, Probe};

const SLICE_PER_SECOND: u64 = 20;

impl Paced {

    pub(super) fn new ( inner: Body, rate: u64, free: u64 ) -> Self {

        Self { inner, rate: rate.max(1), free, sent: 0, started: None, held: None, timer: None }

    }

    pub fn probe ( &mut self, probe: Probe ) {

        self.inner.probe(probe);

    }

    pub fn guard ( &mut self, guard: Guard ) {

        self.inner.guard(guard);

    }

    pub fn is_end ( &self ) -> bool {

        self.held.is_none() && self.inner.is_end_stream()

    }

    pub fn poll ( &mut self, cx: &mut Context<'_> ) -> Poll<Option<Result<Frame<Bytes>, AppError>>> {

        loop {

            if let Some(timer) = &mut self.timer {

                if timer.as_mut().poll(cx).is_pending() { return Poll::Pending; }

                self.timer = None;

            }

            if self.held.is_none() {

                match Pin::new(&mut self.inner).poll_frame(cx) {
                    Poll::Ready(Some(Ok(frame))) => match frame.into_data() {
                        Ok(data) if data.is_empty() => continue,
                        Ok(data) => self.held = Some(data),
                        Err(frame) => return Poll::Ready(Some(Ok(frame))),
                    },
                    other => return other,
                }

            }

            let Some(mut data) = self.held.take() else { continue; };
            let now = Instant::now();
            let started = *self.started.get_or_insert(now);
            let allowed = self.free.saturating_add(self.rate.saturating_mul(now.duration_since(started).as_millis() as u64) / 1_000);
            let budget = allowed.saturating_sub(self.sent);

            if budget == 0 {

                let slice = (self.rate / SLICE_PER_SECOND).clamp(1, data.len() as u64);
                let wait_ms = (self.sent + slice - allowed).saturating_mul(1_000).div_ceil(self.rate).max(1);

                self.held = Some(data);
                self.timer = Some(Box::pin(tokio::time::sleep(Duration::from_millis(wait_ms))));

                continue;

            }

            let take = budget.min(data.len() as u64) as usize;

            if take < data.len() { self.held = Some(data.split_off(take)); }

            self.sent += take as u64;

            return Poll::Ready(Some(Ok(Frame::data(data))));

        }

    }

}
