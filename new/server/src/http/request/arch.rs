use bytes::Bytes;

use crate::http::body::Body;

pub type Req <B = Body> = http::Request<B>;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Request;

pub struct Shadow {
    pub(super) head : Bytes,
    pub(super) body : Option<Box<Body>>,
}
