use bytes::{Bytes, BytesMut};

use super::arch::{Arena, CHUNK};

impl Arena {

    pub fn new () -> Self {

        Self { buffer: BytesMut::with_capacity(CHUNK) }

    }

    pub fn write ( &mut self, room: usize, fill: impl FnOnce(&mut BytesMut) ) -> Bytes {

        self.buffer.reserve(room);

        fill(&mut self.buffer);

        self.buffer.split().freeze()

    }

    pub fn copy ( &mut self, bytes: &[u8] ) -> Bytes {

        self.write(bytes.len(), |buffer| buffer.extend_from_slice(bytes))

    }

}

impl Default for Arena {

    fn default () -> Self {

        Self::new()

    }

}
