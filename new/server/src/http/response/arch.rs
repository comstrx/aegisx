use crate::http::body::Body;

pub type Res <B = Body> = http::Response<B>;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Response;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Abort;
