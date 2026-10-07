use std::pin::Pin;
use std::task::{Context, Poll};

use bytes::{Buf, Bytes};
use http_body::{Body as _, Frame, SizeHint};

use crate::core::error::AppError;
use super::arch::{Inner, Probe, QuicBody};

impl QuicBody {

    pub fn new ( stream: h3::server::RequestStream<h3_quinn::RecvStream, Bytes> ) -> Self {

        Self { stream, data: false, probe: None }

    }

    pub fn probe ( &mut self, probe: Probe ) {

        self.probe = Some(probe);

    }

    pub(super) fn poll ( &mut self, cx: &mut Context<'_> ) -> Poll<Option<Result<Frame<Bytes>, AppError>>> {

        if !self.data {

            match self.stream.poll_recv_data(cx) {
                Poll::Ready(Ok(Some(mut buf))) => {

                    let bytes = buf.copy_to_bytes(buf.remaining());

                    if let Some(probe) = &self.probe { probe.borrow_mut().feed(&bytes); }

                    return Poll::Ready(Some(Ok(Frame::data(bytes))));

                }
                Poll::Ready(Ok(None)) => self.data = true,
                Poll::Ready(Err(error)) => return Poll::Ready(Some(Err(AppError::network("h3 body", error.to_string())))),
                Poll::Pending => return Poll::Pending,
            }

        }

        match self.stream.poll_recv_trailers(cx) {
            Poll::Ready(Ok(Some(map))) => Poll::Ready(Some(Ok(Frame::trailers(map)))),
            Poll::Ready(Ok(None)) => Poll::Ready(None),
            Poll::Ready(Err(error)) => Poll::Ready(Some(Err(AppError::network("h3 trailers", error.to_string())))),
            Poll::Pending => Poll::Pending,
        }

    }

}

impl Inner {

    pub(super) fn poll ( &mut self, cx: &mut Context<'_> ) -> Poll<Option<Result<Frame<Bytes>, AppError>>> {

        match self {
            Self::Hyper(body) => Pin::new(body).poll_frame(cx).map_err(AppError::from),
            Self::Quic(body) => body.poll(cx),
        }

    }

    pub(super) fn is_end_stream ( &self ) -> bool {

        match self {
            Self::Hyper(body) => http_body::Body::is_end_stream(body),
            Self::Quic(body) => body.data,
        }

    }

    pub(super) fn size_hint ( &self ) -> SizeHint {

        match self {
            Self::Hyper(body) => http_body::Body::size_hint(body),
            Self::Quic(_) => SizeHint::default(),
        }

    }

}
