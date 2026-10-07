use std::pin::Pin;
use std::task::{Context, Poll};

use bytes::{Bytes, BytesMut};
use http_body::Frame;
use tokio::fs::File;
use tokio::io::{AsyncRead, ReadBuf};

use crate::core::error::AppError;
use super::arch::{FileBody, Guard, Probe};

const CHUNK: usize = 65_536;

impl FileBody {

    pub fn new ( file: File, length: u64 ) -> Self {

        Self { file, remaining: length, buffer: BytesMut::new(), probe: None, guard: None }

    }

    pub fn probe ( &mut self, probe: Probe ) {

        self.probe = Some(probe);

    }

    pub fn guard ( &mut self, guard: Guard ) {

        self.guard = Some(guard);

    }

    pub fn remaining ( &self ) -> u64 {

        self.remaining

    }

    pub(super) fn poll ( &mut self, cx: &mut Context<'_> ) -> Poll<Option<Result<Frame<Bytes>, AppError>>> {

        if self.remaining == 0 { return Poll::Ready(None); }

        let want = CHUNK.min(usize::try_from(self.remaining).unwrap_or(CHUNK));

        if self.buffer.len() < want { self.buffer.resize(want, 0); }

        let mut read = ReadBuf::new(&mut self.buffer[..want]);

        match Pin::new(&mut self.file).poll_read(cx, &mut read) {
            Poll::Ready(Ok(())) => {

                let filled = read.filled().len();

                if filled == 0 {

                    self.remaining = 0;

                    return Poll::Ready(Some(Err(AppError::http(500, "file shrank while streaming"))));

                }

                self.remaining -= filled as u64;

                let chunk = self.buffer.split_to(filled).freeze();

                if let Some(probe) = &self.probe { let mut probe = probe.borrow_mut(); probe.feed(&chunk); if self.remaining == 0 { probe.finish(); } }

                Poll::Ready(Some(Ok(Frame::data(chunk))))

            }
            Poll::Ready(Err(error)) => Poll::Ready(Some(Err(AppError::from(error)))),
            Poll::Pending => Poll::Pending,
        }

    }

}
