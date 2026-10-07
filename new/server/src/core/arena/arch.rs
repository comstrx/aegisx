use bytes::BytesMut;

pub const CHUNK: usize = 4_096;

pub struct Arena {
    pub(super) buffer : BytesMut,
}
