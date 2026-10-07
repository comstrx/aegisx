use std::cell::RefCell;
use std::future::Future;
use std::path::Path;
use std::pin::Pin;
use std::rc::Rc;
use std::task::{Context, Poll};
use std::time::Duration;

use std::collections::VecDeque;

use bytes::{Bytes, BytesMut};
use tokio::time::{Instant, sleep};
use http_body::{Frame, SizeHint};

use crate::core::error::AppError;
use crate::http::encode::Encoded;
use crate::http::upstream::Streaming;
use super::arch::{Paced, Body, Chained, FileBody, Guard, Incoming, Inner, Limited, Probe, QuicBody, Tap};

impl Body {

    pub fn incoming ( body: Incoming ) -> Self {

        Self::Incoming(body)

    }

    pub fn upstream ( body: Streaming ) -> Self {

        Self::Upstream(Box::new(body))

    }

    pub fn quic ( stream: h3::server::RequestStream<h3_quinn::RecvStream, Bytes> ) -> Self {

        Self::Quic(Box::new(QuicBody::new(stream)))

    }

    pub fn limited ( body: Body, limit: usize, idle_ms: u64 ) -> Self {

        let inner = match body {
            Self::Incoming(body) => Inner::Hyper(body),
            Self::Quic(body) => Inner::Quic(body),
            other => return other,
        };

        Self::Limited(Limited { inner, remaining: limit, idle_ms, timer: None, armed: false, probe: None })

    }

    pub fn replay ( &self ) -> Option<Self> {

        match self {
            Self::Empty => Some(Self::Empty),
            Self::Bytes(data) => Some(Self::Bytes(data.clone())),
            Self::Chunks(chunks) => Some(Self::Chunks(chunks.clone())),
            Self::Incoming(_) | Self::Quic(_) | Self::Upstream(_) | Self::Limited(_) | Self::File(_) | Self::Encoded(_) | Self::Decoded(_) | Self::Replaced(_) | Self::Boxed(_) | Self::Paced(_) | Self::Chained(_) => None,
        }

    }

    pub fn take_upstream ( &mut self ) -> Option<Box<Streaming>> {

        match std::mem::replace(self, Self::Empty) {
            Self::Upstream(body) => Some(body),
            other => { *self = other; None }
        }

    }

    pub async fn gather ( &mut self ) -> Result<(), AppError> {

        use http_body_util::BodyExt;

        let collected = std::mem::replace(self, Self::Empty).collect().await?;

        *self = Self::Bytes(collected.to_bytes());

        Ok(())

    }

    pub async fn spool ( &mut self, memory: usize, dir: &Path ) -> Result<(), AppError> {

        use http_body_util::BodyExt;
        use tokio::io::{AsyncSeekExt, AsyncWriteExt};

        let fail = |error: std::io::Error| AppError::network("spool", error.to_string());
        let mut body = std::mem::replace(self, Self::Empty);
        let mut held = BytesMut::new();
        let mut file: Option<tokio::fs::File> = None;
        let mut length = 0u64;

        while let Some(frame) = body.frame().await {

            let Ok(data) = frame?.into_data() else { continue; };

            length += data.len() as u64;

            match &mut file {
                Some(file) => file.write_all(&data).await.map_err(fail)?,
                None if held.len() + data.len() <= memory => held.extend_from_slice(&data),
                None => {

                    let target = if dir.as_os_str().is_empty() { std::env::temp_dir() } else { dir.to_path_buf() };
                    let created = tokio::task::spawn_blocking(move || tempfile::tempfile_in(target)).await.map_err(|error| AppError::network("spool", error.to_string()))?.map_err(fail)?;
                    let mut created = tokio::fs::File::from_std(created);

                    created.write_all(&held).await.map_err(fail)?;
                    created.write_all(&data).await.map_err(fail)?;

                    held = BytesMut::new();
                    file = Some(created);

                }
            }

        }

        *self = match file {
            Some(mut file) => {

                file.flush().await.map_err(fail)?;
                file.seek(std::io::SeekFrom::Start(0)).await.map_err(fail)?;

                Self::file(file, length)

            }
            None => Self::Bytes(held.freeze()),
        };

        Ok(())

    }

    pub fn chunks ( chunks: VecDeque<Bytes> ) -> Self {

        Self::Chunks(chunks)

    }

    pub fn chained ( head: Bytes, rest: Body ) -> Self {

        Self::Chained(Box::new(Chained { head: Some(head), rest }))

    }

    pub async fn prefix ( body: Body, cap: usize ) -> Result<Body, AppError> {

        use http_body_util::BodyExt;

        let mut body = body;
        let mut gathered = BytesMut::new();

        while gathered.len() < cap {

            let Some(frame) = body.frame().await else { return Ok(Self::Bytes(gathered.freeze())); };

            match frame?.into_data() {
                Ok(data) => gathered.extend_from_slice(&data),
                Err(frame) => { if let Ok(map) = frame.into_trailers() { return Ok(Self::chained(gathered.freeze(), Self::Chunks(VecDeque::new()).trailing(map))); } }
            }

        }

        Ok(Self::chained(gathered.freeze(), body))

    }

    fn trailing ( self, _map: http::HeaderMap ) -> Body {

        self

    }

    pub fn encoded ( body: Encoded ) -> Self {

        Self::Encoded(Box::new(body))

    }

    pub fn paced ( body: Body, rate: u64, free: u64 ) -> Self {

        Self::Paced(Box::new(Paced::new(body, rate, free)))

    }

    pub fn file ( file: tokio::fs::File, length: u64 ) -> Self {

        Self::File(Box::new(FileBody::new(file, length)))

    }

    pub fn bytes ( data: impl Into<Bytes> ) -> Self {

        Self::Bytes(data.into())

    }

    pub fn empty () -> Self {

        Self::Empty

    }

    pub fn guard ( &mut self, guard: Guard ) {

        match self {
            Self::Upstream(body) => body.guard(guard),
            Self::File(body) => body.guard(guard),
            Self::Encoded(body) => body.guard(guard),
            Self::Decoded(body) => body.guard(guard),
            Self::Replaced(body) => body.guard(guard),
            Self::Boxed(body) => body.guard(guard),
            Self::Paced(body) => body.guard(guard),
            Self::Chained(body) => body.rest.guard(guard),
            Self::Incoming(_) | Self::Quic(_) | Self::Limited(_) | Self::Bytes(_) | Self::Chunks(_) | Self::Empty => {}
        }

    }

    pub fn keeps ( &self ) -> bool {

        match self {
            Self::Upstream(_) | Self::File(_) | Self::Encoded(_) | Self::Decoded(_) | Self::Replaced(_) | Self::Boxed(_) | Self::Paced(_) => true,
            Self::Chained(body) => body.rest.keeps(),
            Self::Incoming(_) | Self::Quic(_) | Self::Limited(_) | Self::Bytes(_) | Self::Chunks(_) | Self::Empty => false,
        }

    }

    pub fn probe ( &mut self, probe: Probe ) {

        match self {
            Self::Upstream(body) => body.probe(probe),
            Self::File(body) => body.probe(probe),
            Self::Encoded(body) => body.probe(probe),
            Self::Decoded(body) => body.probe(probe),
            Self::Replaced(body) => body.probe(probe),
            Self::Boxed(body) => body.probe(probe),
            Self::Paced(body) => body.probe(probe),
            Self::Chained(body) => { if let Some(head) = &body.head { probe.borrow_mut().feed(head); } body.rest.probe(probe); }
            Self::Limited(limited) => limited.probe = Some(probe),
            Self::Quic(body) => body.probe(probe),
            Self::Bytes(data) => { let mut tap = probe.borrow_mut(); tap.feed(data); tap.finish(); }
            Self::Chunks(chunks) => { let mut tap = probe.borrow_mut(); for chunk in chunks.iter() { tap.feed(chunk); } tap.finish(); }
            Self::Incoming(_) | Self::Empty => {}
        }

    }

    pub async fn drain ( body: Body, cap: usize ) {

        use http_body_util::BodyExt;

        let mut body = body;
        let mut seen = 0usize;

        while let Some(frame) = body.frame().await {

            let Ok(frame) = frame else { return; };

            if let Some(data) = frame.data_ref() {

                seen += data.len();

                if seen > cap { return; }

            }

        }

    }

    fn exceeded () -> AppError {

        AppError::http(413, "request body exceeds the configured limit")

    }

}

impl Tap {

    pub fn new ( cap: usize ) -> Probe {

        Rc::new(RefCell::new(Self { buffer: Vec::new(), cap, seen: 0, done: false }))

    }

    pub fn feed ( &mut self, bytes: &[u8] ) {

        let room = self.cap.saturating_sub(self.buffer.len());

        self.buffer.extend_from_slice(&bytes[..bytes.len().min(room)]);
        self.seen = self.seen.saturating_add(bytes.len());

    }

    pub fn seen ( &self ) -> usize {

        self.seen

    }

    pub fn finish ( &mut self ) {

        self.done = true;

    }

    pub fn complete ( &self ) -> bool {

        self.done

    }

    pub fn whole ( &self ) -> Option<&[u8]> {

        (self.done && self.seen == self.buffer.len()).then_some(self.buffer.as_slice())

    }

    pub fn take ( &mut self ) -> ( Vec<u8>, usize ) {

        ( std::mem::take(&mut self.buffer), self.seen )

    }

}

impl Limited {

    fn poll ( &mut self, cx: &mut Context<'_> ) -> Poll<Option<Result<Frame<Bytes>, AppError>>> {

        let frame = match self.inner.poll(cx) {
            Poll::Ready(Some(Ok(frame))) => { self.armed = false; frame }
            Poll::Ready(Some(Err(error))) => return Poll::Ready(Some(Err(error))),
            Poll::Ready(None) => return Poll::Ready(None),
            Poll::Pending => {

                if self.idle_ms == 0 { return Poll::Pending; }

                let idle = Duration::from_millis(self.idle_ms);
                let timer = self.timer.get_or_insert_with(|| Box::pin(sleep(idle)));

                if !self.armed { timer.as_mut().reset(Instant::now() + idle); self.armed = true; }

                if timer.as_mut().poll(cx).is_ready() { return Poll::Ready(Some(Err(AppError::http(408, "client body timed out")))); }

                return Poll::Pending;

            }
        };

        if let Some(data) = frame.data_ref() {

            match self.remaining.checked_sub(data.len()) {
                Some(left) => self.remaining = left,
                None => return Poll::Ready(Some(Err(Body::exceeded()))),
            }

            if let Some(probe) = &self.probe { probe.borrow_mut().feed(data); }

        }

        Poll::Ready(Some(Ok(frame)))

    }

}

impl http_body::Body for Body {

    type Data = Bytes;
    type Error = AppError;

    fn poll_frame ( self: Pin<&mut Self>, cx: &mut Context<'_> ) -> Poll<Option<Result<Frame<Bytes>, AppError>>> {

        let this = self.get_mut();

        match this {
            Self::Incoming(body) => Pin::new(body).poll_frame(cx).map_err(AppError::from),
            Self::Quic(body) => body.poll(cx),
            Self::Upstream(body) => Pin::new(body.as_mut()).poll_frame(cx),
            Self::Limited(limited) => limited.poll(cx),
            Self::File(body) => body.poll(cx),
            Self::Encoded(body) => Pin::new(body.as_mut()).poll_frame(cx),
            Self::Decoded(body) => Pin::new(body.as_mut()).poll_frame(cx),
            Self::Replaced(body) => Pin::new(body.as_mut()).poll_frame(cx),
            Self::Boxed(body) => body.frame(cx),
            Self::Paced(body) => body.poll(cx),
            Self::Chained(body) => {

                if let Some(head) = body.head.take() { return Poll::Ready(Some(Ok(Frame::data(head)))); }

                Pin::new(&mut body.rest).poll_frame(cx)

            }
            Self::Chunks(chunks) => match chunks.pop_front() { Some(chunk) => Poll::Ready(Some(Ok(Frame::data(chunk)))), None => Poll::Ready(None) },
            Self::Bytes(data) => {

                let data = std::mem::take(data);

                *this = Self::Empty;

                if data.is_empty() { Poll::Ready(None) } else { Poll::Ready(Some(Ok(Frame::data(data)))) }

            }
            Self::Empty => Poll::Ready(None),
        }

    }

    fn is_end_stream ( &self ) -> bool {

        match self {
            Self::Incoming(body) => body.is_end_stream(),
            Self::Quic(body) => body.data,
            Self::Upstream(body) => body.is_end(),
            Self::Limited(limited) => limited.inner.is_end_stream(),
            Self::File(body) => body.remaining() == 0,
            Self::Encoded(body) => body.is_end(),
            Self::Decoded(body) => body.is_end(),
            Self::Replaced(body) => body.is_end(),
            Self::Boxed(body) => body.ended(),
            Self::Paced(body) => body.is_end(),
            Self::Chained(body) => body.head.is_none() && body.rest.is_end_stream(),
            Self::Chunks(chunks) => chunks.is_empty(),
            Self::Bytes(data) => data.is_empty(),
            Self::Empty => true,
        }

    }

    fn size_hint ( &self ) -> SizeHint {

        match self {
            Self::Incoming(body) => body.size_hint(),
            Self::Quic(_) => SizeHint::default(),
            Self::Upstream(body) => body.hint(),
            Self::Limited(limited) => limited.inner.size_hint(),
            Self::File(body) => SizeHint::with_exact(body.remaining()),
            Self::Encoded(_) | Self::Decoded(_) | Self::Replaced(_) | Self::Boxed(_) => SizeHint::default(),
            Self::Paced(body) => { let mut hint = body.inner.size_hint(); let held = body.held.as_ref().map_or(0, |held| held.len() as u64); if let Some(upper) = hint.upper() { hint.set_upper(upper + held); } hint.set_lower(hint.lower() + held); hint }
            Self::Chained(body) => { let inner = body.rest.size_hint(); let head = body.head.as_ref().map_or(0, |head| head.len() as u64); let mut hint = SizeHint::new(); if let Some(upper) = inner.upper() { hint.set_upper(upper + head); } hint.set_lower(inner.lower() + head); hint }
            Self::Chunks(chunks) => SizeHint::with_exact(chunks.iter().map(|chunk| chunk.len() as u64).sum()),
            Self::Bytes(data) => SizeHint::with_exact(data.len() as u64),
            Self::Empty => SizeHint::with_exact(0),
        }

    }

}
