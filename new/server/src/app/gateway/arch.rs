use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::Arc;
use std::time::Instant;

use bytes::Bytes;
use http::Uri;
use http::header::{HeaderName, HeaderValue};

use crate::app::{Actor, Analyser, Backend, Capture, Certificate, Claim, Decisions, Entry, Fill, History, Hooks, Journal, Key, Lease, Seen, Memory, Peer, Pending, Picker, PoolState, RouteState, Runtime, Stats, Store, Stored, Trace};
use crate::core::arena::Arena;
use crate::core::rand::Rng;
use crate::core::sync::{Lens, Local, Shared};
use crate::http::body::{Body, Probe};
use crate::http::encode::Encoding;
use crate::http::fastcgi::Fcgi;
use crate::http::files::FileCache;
use crate::http::io::Upgrading;
use crate::http::request::Req;
use crate::http::response::Res;
use crate::http::upstream::Client;

pub struct Handler {
    pub(super) runtime   : Lens<Runtime>,
    pub(super) client    : Client,
    pub(super) scripts   : Fcgi,
    pub(super) hooks     : Local<( u64, Option<Rc<Hooks>> )>,
    pub(super) picker    : Local<Picker>,
    pub(super) rng       : Local<Rng>,
    pub(super) arena     : Local<Arena>,
    pub(super) stats     : Arc<Stats>,
    pub(super) capture   : Shared<Capture>,
    pub(super) analyser  : Option<Arc<Analyser>>,
    pub(super) memory    : Arc<Memory>,
    pub(super) decisions : Option<Arc<Decisions>>,
    pub(super) access    : Option<Journal>,
    pub(super) files     : Arc<FileCache>,
    pub(super) cache     : Option<Arc<Store>>,
    pub(super) worker    : usize,
    pub(super) tunnels   : Rc<Cell<usize>>,
}

pub struct Bridged {
    pub(super) count : Rc<Cell<usize>>,
}

pub struct Sinks {
    pub stats    : Arc<Stats>,
    pub capture  : Shared<Capture>,
    pub analyser : Option<Arc<Analyser>>,
    pub access   : Option<Journal>,
}

#[repr(C)]
pub struct Context {
    pub(super) seen     : Cell<Instant>,
    pub(super) inflight : Cell<u32>,
    pub(super) served   : Cell<bool>,
    pub(super) count    : Cell<u32>,
    pub(super) born     : Instant,
    pub(super) stats    : Arc<Stats>,
    pub peer            : Peer,
    pub(super) client   : RefCell<Option<Box<Certificate>>>,
    pub(super) actor    : RefCell<Option<( Option<HeaderValue>, Key )>>,
}

pub struct Ticket {
    pub(super) context : Rc<Context>,
    pub(super) counted : bool,
    pub(super) lease   : Option<Lease>,
    pub(super) actor   : Option<Arc<Actor>>,
    pub(super) pending : Option<Box<Pending>>,
    pub(super) access  : Option<( Journal, Box<Entry> )>,
    pub(super) fill    : Option<Box<Fill>>,
}

pub type Probing = ( [f32; 16], Probe, Probe );

#[derive(Default)]
pub struct Served {
    pub(super) route   : Option<usize>,
    pub(super) proxied : bool,
}

pub struct Flight <'a> {
    pub(super) runtime  : &'a Runtime,
    pub(super) route    : &'a RouteState,
    pub(super) context  : &'a Rc<Context>,
    pub(super) ledger   : &'a mut Option<Box<Entry>>,
    pub(super) served   : &'a mut Served,
    pub(super) started  : Instant,
    pub(super) id       : HeaderValue,
    pub(super) ticket   : Option<Ticket>,
    pub(super) rare     : Option<Box<Rare>>,
    pub(super) declared : u64,
    pub(super) encoding : Option<Encoding>,
    pub(super) observe  : bool,
    pub(super) gunzip   : bool,
}

#[derive(Default)]
pub struct Rare {
    pub(super) canonical    : Option<String>,
    pub(super) rewritten    : Option<String>,
    pub(super) trace        : Option<Trace>,
    pub(super) watched      : Option<( Key, Arc<Actor>, History )>,
    pub(super) probe        : Option<Box<Probing>>,
    pub(super) filling      : Option<Bytes>,
    pub(super) revalidating : Option<Arc<Stored>>,
    pub(super) fallback     : Option<Arc<Stored>>,
    pub(super) vars         : Option<Box<Vars>>,
    pub(super) upgrade      : Option<HeaderValue>,
    pub(super) upgrading    : Option<Upgrading>,
    pub(super) chain        : Option<HeaderValue>,
    pub(super) rendered     : Vec<( HeaderName, HeaderValue )>,
    pub(super) claim        : Option<Claim>,
    pub(super) gauge        : Option<Arc<Actor>>,
    pub(super) passing      : bool,
    pub(super) seen         : Option<Box<Seen>>,
    pub(super) satisfied    : bool,
}

pub struct Hop <'p> {
    pub(super) pool       : &'p PoolState,
    pub(super) backend    : &'p Arc<Backend>,
    pub(super) lease      : Option<Lease>,
    pub(super) stuck      : Option<usize>,
    pub(super) attempt    : usize,
    pub(super) elapsed_us : u64,
}

#[derive(Clone, Copy)]
pub struct Flow {
    pub(super) head      : bool,
    pub(super) retryable : bool,
    pub(super) more      : bool,
    pub(super) now       : u64,
}

pub type Landed <'p> = Result<( Res<Body>, Hop<'p>, bool ), ( Option<Box<Req<Body>>>, Option<u16> )>;

pub struct Vars {
    pub(super) host    : Option<HeaderValue>,
    pub(super) uri     : Uri,
    pub(super) derived : Vec<Bytes>,
}
