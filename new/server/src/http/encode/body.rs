use std::io::Write;
use std::pin::Pin;
use std::task::{Context, Poll};

use bytes::{BufMut, Bytes, BytesMut};
use flate2::Compression as Level;
use flate2::write::GzEncoder;
use http_body::{Frame, SizeHint};

use crate::core::error::AppError;
use crate::http::body::{Body, Guard, Probe};
use super::arch::{Codec, Encoded, Encoding};

const THRESHOLD: usize = 16_384;
const BROTLI_BUFFER: usize = 4_096;
const BROTLI_WINDOW: u32 = 22;

impl Codec {

    pub(super) fn new ( encoding: Encoding, level: u32, brotli_level: u32, zstd_level: i32 ) -> std::io::Result<Self> {

        let sink = BytesMut::with_capacity(THRESHOLD).writer();

        Ok(match encoding {
            Encoding::Gzip => Self::Gzip(Box::new(GzEncoder::new(sink, Level::new(level)))),
            Encoding::Brotli => Self::Brotli(Box::new(brotli::CompressorWriter::new(sink, BROTLI_BUFFER, brotli_level, BROTLI_WINDOW))),
            Encoding::Zstd => Self::Zstd(Box::new(zstd::stream::write::Encoder::new(sink, zstd_level)?)),
        })

    }

    fn write ( &mut self, data: &[u8] ) -> std::io::Result<()> {

        match self { Self::Gzip(encoder) => encoder.write_all(data), Self::Brotli(encoder) => encoder.write_all(data), Self::Zstd(encoder) => encoder.write_all(data) }

    }

    fn flush ( &mut self ) -> std::io::Result<()> {

        match self { Self::Gzip(encoder) => encoder.flush(), Self::Brotli(encoder) => encoder.flush(), Self::Zstd(encoder) => encoder.flush() }

    }

    fn out ( &mut self ) -> &mut BytesMut {

        match self { Self::Gzip(encoder) => encoder.get_mut().get_mut(), Self::Brotli(encoder) => encoder.get_mut().get_mut(), Self::Zstd(encoder) => encoder.get_mut().get_mut() }

    }

    fn finish ( self ) -> std::io::Result<BytesMut> {

        match self {
            Self::Gzip(encoder) => Ok((*encoder).finish()?.into_inner()),
            Self::Brotli(encoder) => Ok((*encoder).into_inner().into_inner()),
            Self::Zstd(encoder) => Ok((*encoder).finish()?.into_inner()),
        }

    }

}

impl Encoded {

    pub(super) fn new ( inner: Body, codec: Codec ) -> Self {

        Self { inner, codec: Some(codec), tail: BytesMut::new(), dirty: false, trailers: None, probe: None, guard: None }

    }

    pub fn probe ( &mut self, probe: Probe ) {

        self.probe = Some(probe);

    }

    pub fn guard ( &mut self, guard: Guard ) {

        self.guard = Some(guard);

    }

    pub fn is_end ( &self ) -> bool {

        self.codec.is_none() && self.tail.is_empty() && self.trailers.is_none()

    }

    fn emit ( &mut self, chunk: Bytes ) -> Poll<Option<Result<Frame<Bytes>, AppError>>> {

        if let Some(probe) = &self.probe { probe.borrow_mut().feed(&chunk); }

        Poll::Ready(Some(Ok(Frame::data(chunk))))

    }

}

impl http_body::Body for Encoded {

    type Data = Bytes;
    type Error = AppError;

    fn poll_frame ( self: Pin<&mut Self>, cx: &mut Context<'_> ) -> Poll<Option<Result<Frame<Bytes>, AppError>>> {

        let this = self.get_mut();

        loop {

            let Some(codec) = this.codec.as_mut() else {

                if !this.tail.is_empty() { let chunk = this.tail.split().freeze(); return this.emit(chunk); }

                if let Some(trailers) = this.trailers.take() { return Poll::Ready(Some(Ok(Frame::trailers(trailers)))); }

                if let Some(probe) = &this.probe { probe.borrow_mut().finish(); }

                return Poll::Ready(None);

            };

            match Pin::new(&mut this.inner).poll_frame(cx) {
                Poll::Ready(Some(Ok(frame))) => match frame.into_data() {
                    Ok(data) => {

                        if let Err(error) = codec.write(&data) { return Poll::Ready(Some(Err(AppError::from(error)))); }

                        this.dirty = true;

                        if codec.out().len() >= THRESHOLD { let chunk = codec.out().split().freeze(); return this.emit(chunk); }

                    }
                    Err(frame) => { if let Ok(trailers) = frame.into_trailers() { this.trailers = Some(trailers); } }
                },
                Poll::Ready(Some(Err(error))) => return Poll::Ready(Some(Err(error))),
                Poll::Ready(None) => {

                    let Some(codec) = this.codec.take() else { continue; };

                    match codec.finish() {
                        Ok(tail) => this.tail = tail,
                        Err(error) => return Poll::Ready(Some(Err(AppError::from(error)))),
                    }

                }
                Poll::Pending => {

                    if this.dirty {

                        if let Err(error) = codec.flush() { return Poll::Ready(Some(Err(AppError::from(error)))); }

                        this.dirty = false;

                    }

                    if codec.out().is_empty() { return Poll::Pending; }

                    let chunk = codec.out().split().freeze();

                    return this.emit(chunk);

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
