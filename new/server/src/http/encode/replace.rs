use std::pin::Pin;
use std::task::{Context, Poll};

use bytes::{Bytes, BytesMut};
use http::header::{ACCEPT_RANGES, CONTENT_ENCODING, CONTENT_LENGTH, CONTENT_TYPE, ETAG};
use http_body::Body as _;
use http_body::{Frame, SizeHint};

use crate::core::error::AppError;
use crate::http::body::{Body, Guard, Probe};
use crate::http::response::Res;
use super::arch::{Replaced, Swaps};

impl Replaced {

    pub fn apply ( swaps: &Swaps, response: &mut Res ) -> bool {

        let Swaps { rules, types } = swaps;


        if response.body().is_end_stream() || response.headers().contains_key(CONTENT_ENCODING) { return false; }

        if !response.headers().get(CONTENT_TYPE).is_some_and(|value| types.iter().any(|kind| value.as_bytes().len() >= kind.len() && value.as_bytes()[..kind.len()].eq_ignore_ascii_case(kind.as_bytes()))) { return false; }

        let headers = response.headers_mut();

        headers.remove(CONTENT_LENGTH);
        headers.remove(ACCEPT_RANGES);
        headers.remove(ETAG);

        let inner = std::mem::replace(response.body_mut(), Body::Empty);
        let longest = rules.iter().map(|( pattern, _ )| pattern.len()).max().unwrap_or(1);

        *response.body_mut() = Body::Replaced(Box::new(Self { inner, rules: rules.clone(), carry: BytesMut::new(), longest, ended: false, trailers: None, probe: None, guard: None }));

        true

    }

    pub fn probe ( &mut self, probe: Probe ) {

        self.probe = Some(probe);

    }

    pub fn guard ( &mut self, guard: Guard ) {

        self.guard = Some(guard);

    }

    pub fn is_end ( &self ) -> bool {

        self.ended && self.carry.is_empty() && self.trailers.is_none()

    }

    fn rewrite ( &mut self, last: bool ) -> Bytes {

        let buffer = std::mem::take(&mut self.carry);
        let mut out = BytesMut::with_capacity(buffer.len() + 64);
        let mut position = 0;

        while let Some(( at, pattern, replacement )) = self.rules.iter().filter_map(|( pattern, replacement )| memchr::memmem::find(&buffer[position..], pattern).map(|at| ( position + at, pattern, replacement ))).min_by_key(|( at, _, _ )| *at) {

            out.extend_from_slice(&buffer[position..at]);
            out.extend_from_slice(replacement);
            position = at + pattern.len();

        }

        let safe = if last { buffer.len() } else { buffer.len().saturating_sub(self.longest.saturating_sub(1)).max(position) };

        out.extend_from_slice(&buffer[position..safe]);
        self.carry.extend_from_slice(&buffer[safe..]);

        out.freeze()

    }

    fn emit ( &mut self, chunk: Bytes ) -> Poll<Option<Result<Frame<Bytes>, AppError>>> {

        if let Some(probe) = &self.probe { probe.borrow_mut().feed(&chunk); }

        Poll::Ready(Some(Ok(Frame::data(chunk))))

    }

}

impl http_body::Body for Replaced {

    type Data = Bytes;
    type Error = AppError;

    fn poll_frame ( self: Pin<&mut Self>, cx: &mut Context<'_> ) -> Poll<Option<Result<Frame<Bytes>, AppError>>> {

        let this = self.get_mut();

        loop {

            if this.ended {

                if !this.carry.is_empty() { let rest = this.rewrite(true); if !rest.is_empty() { return this.emit(rest); } }

                if let Some(trailers) = this.trailers.take() { return Poll::Ready(Some(Ok(Frame::trailers(trailers)))); }

                if let Some(probe) = &this.probe { probe.borrow_mut().finish(); }

                return Poll::Ready(None);

            }

            match Pin::new(&mut this.inner).poll_frame(cx) {
                Poll::Ready(Some(Ok(frame))) => match frame.into_data() {
                    Ok(data) => {

                        this.carry.extend_from_slice(&data);

                        let ready = this.rewrite(false);

                        if !ready.is_empty() { return this.emit(ready); }

                    }
                    Err(frame) => { this.trailers = frame.into_trailers().ok(); }
                },
                Poll::Ready(Some(Err(error))) => return Poll::Ready(Some(Err(error))),
                Poll::Ready(None) => this.ended = true,
                Poll::Pending => return Poll::Pending,
            }

        }

    }

    fn is_end_stream ( &self ) -> bool {

        self.is_end()

    }

    fn size_hint ( &self ) -> SizeHint {

        SizeHint::default()

    }

}
