use std::io::Write;
use std::pin::Pin;
use std::task::{Context, Poll};

use bytes::{BufMut, Bytes, BytesMut};
use http::header::{ACCEPT_RANGES, CONTENT_ENCODING, CONTENT_LENGTH, ETAG, HeaderValue};
use http_body::Body as _;
use http_body::{Frame, SizeHint};

use crate::core::error::AppError;
use crate::http::body::{Body, Guard, Probe};
use crate::http::response::Res;
use super::arch::{Compression, Decoded};

const THRESHOLD: usize = 16_384;

impl Compression {

    pub fn gunzip ( &self, response: &mut Res ) -> bool {

        if response.body().is_end_stream() || !response.headers().get(CONTENT_ENCODING).is_some_and(|value| value.as_bytes().eq_ignore_ascii_case(b"gzip")) { return false; }

        let headers = response.headers_mut();

        headers.remove(CONTENT_ENCODING);
        headers.remove(CONTENT_LENGTH);
        headers.remove(ACCEPT_RANGES);

        if let Some(etag) = headers.get(ETAG) && etag.as_bytes().first() == Some(&b'"') {

            let mut weak = Vec::with_capacity(etag.len() + 2);

            weak.extend_from_slice(b"W/");
            weak.extend_from_slice(etag.as_bytes());

            if let Ok(value) = HeaderValue::from_bytes(&weak) { headers.insert(ETAG, value); }

        }

        let inner = std::mem::replace(response.body_mut(), Body::Empty);
        let decoder = flate2::write::GzDecoder::new(BytesMut::with_capacity(THRESHOLD).writer());

        *response.body_mut() = Body::Decoded(Box::new(Decoded { inner, decoder: Some(Box::new(decoder)), trailers: None, probe: None, guard: None }));

        true

    }

}

impl Decoded {

    pub fn probe ( &mut self, probe: Probe ) {

        self.probe = Some(probe);

    }

    pub fn guard ( &mut self, guard: Guard ) {

        self.guard = Some(guard);

    }

    pub fn is_end ( &self ) -> bool {

        self.decoder.is_none() && self.trailers.is_none()

    }

    fn emit ( &mut self, chunk: Bytes ) -> Poll<Option<Result<Frame<Bytes>, AppError>>> {

        if let Some(probe) = &self.probe { probe.borrow_mut().feed(&chunk); }

        Poll::Ready(Some(Ok(Frame::data(chunk))))

    }

    fn drain ( &mut self, minimum: usize ) -> Option<Bytes> {

        let out = self.decoder.as_mut()?.get_mut().get_mut();

        (out.len() >= minimum && !out.is_empty()).then(|| out.split().freeze())

    }

}

impl http_body::Body for Decoded {

    type Data = Bytes;
    type Error = AppError;

    fn poll_frame ( self: Pin<&mut Self>, cx: &mut Context<'_> ) -> Poll<Option<Result<Frame<Bytes>, AppError>>> {

        let this = self.get_mut();

        loop {

            if this.decoder.is_none() {

                if let Some(trailers) = this.trailers.take() { return Poll::Ready(Some(Ok(Frame::trailers(trailers)))); }

                if let Some(probe) = &this.probe { probe.borrow_mut().finish(); }

                return Poll::Ready(None);

            }

            if let Some(chunk) = this.drain(THRESHOLD) { return this.emit(chunk); }

            match Pin::new(&mut this.inner).poll_frame(cx) {
                Poll::Ready(Some(Ok(frame))) => match frame.into_data() {
                    Ok(data) => { if let Some(decoder) = this.decoder.as_mut() && let Err(error) = decoder.write_all(&data) { return Poll::Ready(Some(Err(AppError::message(format!("gunzip failed: {error}"))))); } }
                    Err(frame) => { this.trailers = frame.into_trailers().ok(); }
                },
                Poll::Ready(Some(Err(error))) => return Poll::Ready(Some(Err(error))),
                Poll::Ready(None) => {

                    let Some(decoder) = this.decoder.take() else { continue; };
                    let out = match (*decoder).finish() { Ok(writer) => writer.into_inner(), Err(error) => return Poll::Ready(Some(Err(AppError::message(format!("gunzip failed: {error}"))))) };

                    if !out.is_empty() { return this.emit(out.freeze()); }

                }
                Poll::Pending => {

                    if let Some(decoder) = this.decoder.as_mut() { let _ = decoder.flush(); }

                    if let Some(chunk) = this.drain(1) { return this.emit(chunk); }

                    return Poll::Pending;

                }
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
