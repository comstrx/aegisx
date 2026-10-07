#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HashKey {
    Ip,
    Uri,
    Host,
    Header(http::header::HeaderName),
    Cookie(Box<str>),
    Query(Box<str>),
}

#[derive(Clone, Copy)]
pub struct Hint <'a> {
    pub ip      : std::net::IpAddr,
    pub secure  : bool,
    pub headers : &'a http::HeaderMap,
    pub uri     : &'a http::Uri,
}
