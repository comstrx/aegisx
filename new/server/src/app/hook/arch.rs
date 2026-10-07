use std::cell::Cell;
use std::net::IpAddr;
use std::rc::Rc;

use http::header::HeaderValue;
use http::{Method, Uri};

pub struct Hooks {
    pub(super) lua      : mlua::Lua,
    pub(super) request  : Option<mlua::Function>,
    pub(super) response : Option<mlua::Function>,
    pub(super) spent    : Rc<Cell<u32>>,
}

#[derive(Clone, Copy)]
pub struct View <'a> {
    pub method : &'a Method,
    pub uri    : &'a Uri,
    pub host   : Option<&'a HeaderValue>,
    pub ip     : IpAddr,
    pub secure : bool,
}

pub struct Seen {
    pub method : Method,
    pub uri    : Uri,
    pub host   : Option<HeaderValue>,
}
