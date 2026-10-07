use std::cell::Cell;
use std::io::ErrorKind;
use std::rc::Rc;
use std::task::{Context, Poll};
use std::time::{Duration, Instant};

use bytes::{Bytes, BytesMut};
use fastcgi_client::response::Content;
use fastcgi_client::{Client, ClientError, Params, Request};
use futures_util::{FutureExt, StreamExt, TryStreamExt};
use http::StatusCode;
use http::header::{CONTENT_TYPE, HOST, HeaderName, HeaderValue, LOCATION};
use http_body::{Body as _, Frame};
use http_body_util::BodyDataStream;
use tokio::io::{AsyncReadExt, BufWriter};
use tokio_io_timeout::TimeoutStream;
use tokio_util::io::StreamReader;

use crate::core::error::{AppError, AppResult};
use crate::core::log::warn;
use crate::core::net::Address;
use crate::core::rt::Rt;
use crate::core::sync::Local;
use crate::http::body::{Body, Frames, Guard, Probe};
use crate::http::request::Req;
use crate::http::response::Res;
use crate::http::server::Stream;
use crate::http::upstream::{Failure, Settings, Upstream};
use super::arch::{Call, Chunks, Fcgi, Link, Output, Script};

const HEAD_MAX: usize = 65_536;
const RECORD: usize = 65_544;

impl Script {

    pub fn covers ( &self, path: &str ) -> bool {

        self.split(path).is_some()

    }

    fn split <'p> ( &self, path: &'p str ) -> Option<( &'p str, &'p str )> {

        let end = path.find(&*self.suffix)? + self.suffix.len();

        (end == path.len() || path.as_bytes()[end] == b'/').then(|| ( &path[..end], &path[end..] ))

    }

}

impl Fcgi {

    pub fn new ( settings: Settings ) -> Self {

        Self { idle: Local::new(Vec::new()), idle_ms: settings.pool_idle_ms.max(1), sweeping: Rc::new(Cell::new(false)) }

    }

    pub async fn exchange ( &self, upstream: &Upstream, call: Call<'_>, request: Box<Req<Body>> ) -> Result<Res<Body>, Failure> {

        let mut request = request;
        let fail = |error: AppError| Failure { error, request: None, connect: false };

        let length = match request.body().size_hint().exact() {
            Some(length) => length,
            None => {

                request.body_mut().gather().await.map_err(fail)?;

                request.body().size_hint().exact().unwrap_or(0)

            }
        };

        let ( link, reused ) = match self.take(upstream.slot) {
            Some(link) => ( link, true ),
            None => match Self::dial(&upstream.addr, call.timeout_ms).await {
                Ok(link) => ( link, false ),
                Err(error) => return Err(Failure { error, request: Some(request), connect: true }),
            },
        };

        let params = Self::params(call, &request, length);
        let body = std::mem::replace(request.body_mut(), Body::Empty);

        let again = match &body {
            Body::Bytes(data) if reused => Some(( params.clone(), Body::Bytes(data.clone()) )),
            Body::Empty if reused => Some(( params.clone(), Body::Empty )),
            _ => None,
        };

        match ( self.answer(link, params, body, upstream.slot, call).await, again ) {
            ( Err(_), Some(( params, body )) ) => {

                let link = Self::dial(&upstream.addr, call.timeout_ms).await.map_err(fail)?;

                self.answer(link, params, body, upstream.slot, call).await.map_err(fail)

            }
            ( outcome, _ ) => outcome.map_err(fail),
        }

    }

    async fn dial ( address: &Address, timeout_ms: u64 ) -> AppResult<Link> {

        Ok(Box::pin(TimeoutStream::new(BufWriter::with_capacity(RECORD, Stream::dial(address, timeout_ms).await?))))

    }

    fn take ( &self, slot: usize ) -> Option<Link> {

        self.idle.with_mut(|lists| {

            let list = lists.get_mut(slot)?;

            while let Some(( mut link, since )) = list.pop() {

                if since.elapsed().as_millis() < u128::from(self.idle_ms) && link.read(&mut [0u8; 1]).now_or_never().is_none() { return Some(link); }

            }

            None

        })

    }

    fn park ( &self, slot: usize, link: Link, keep: usize ) {

        self.idle.with_mut(|lists| {

            if lists.len() <= slot { lists.resize_with(slot + 1, Vec::new); }

            if lists[slot].len() < keep { lists[slot].push(( link, Instant::now() )); }

        });

        if !self.sweeping.replace(true) { Rt::spawn_local(self.clone().sweep()); }

    }

    async fn sweep ( self ) {

        loop {

            Rt::sleep(self.idle_ms).await;

            let left = self.idle.with_mut(|lists| lists.iter_mut().map(|list| { list.retain(|( _, since )| since.elapsed().as_millis() < u128::from(self.idle_ms)); list.len() }).sum::<usize>());

            if left == 0 { self.sweeping.set(false); return; }

        }

    }

    async fn answer ( &self, link: Link, params: Params<'static>, body: Body, slot: usize, call: Call<'_> ) -> AppResult<Res<Body>> {

        let mut chunks: Chunks = Box::pin(self.clone().run(link, params, body, slot, call.keep, call.timeout_ms));
        let mut head = BytesMut::new();

        loop {

            match chunks.next().await {
                Some(Ok(chunk)) => head.extend_from_slice(&chunk),
                Some(Err(error)) => return Err(error),
                None => return Err(AppError::http(502, "fastcgi response ended before its head")),
            }

            if let Some(( mut response, consumed )) = Self::head(&head)? {

                let rest = head.split_off(consumed).freeze();

                *response.body_mut() = Body::Boxed(Box::new(Output { chunks, rest: Some(rest).filter(|rest| !rest.is_empty()), done: false, probe: None, guard: None }));

                return Ok(response);

            }

            if head.len() > HEAD_MAX { return Err(AppError::http(502, "fastcgi response head is too large")); }

        }

    }

    fn run ( self, link: Link, params: Params<'static>, body: Body, slot: usize, keep: usize, timeout_ms: u64 ) -> impl futures_util::Stream<Item = AppResult<Bytes>> {

        async_stream::stream! {

            let mut link = link;
            let idle = Some(Duration::from_millis(timeout_ms.max(1)));

            link.as_mut().set_read_timeout_pinned(idle);
            link.as_mut().set_write_timeout_pinned(idle);

            {

                let stdin = StreamReader::new(BodyDataStream::new(body).map_err(std::io::Error::other));
                let mut client = Client::new_keep_alive_tokio(&mut link);

                let mut output = match client.execute_stream(Request::new_tokio(params, stdin)).await {
                    Ok(output) => output,
                    Err(error) => { yield Err(Self::cause(error, timeout_ms)); return; }
                };

                while let Some(content) = output.next().await {

                    match content {
                        Ok(Content::Stdout(chunk)) => yield Ok(chunk),
                        Ok(Content::Stderr(text)) => warn!(message = %String::from_utf8_lossy(&text).trim_end(), "fastcgi stderr"),
                        Err(error) => { yield Err(Self::cause(error, timeout_ms)); return; }
                    }

                }

            }

            if keep > 0 { self.park(slot, link, keep); }

        }

    }

    fn cause ( error: ClientError, timeout_ms: u64 ) -> AppError {

        match error {
            ClientError::Io(error) if error.kind() == ErrorKind::TimedOut => AppError::timeout("fastcgi", timeout_ms),
            ClientError::Io(error) => error.downcast::<AppError>().unwrap_or_else(|error| AppError::network("fastcgi", error.to_string())),
            other => AppError::http(502, format!("fastcgi: {other}")),
        }

    }

    fn params ( call: Call<'_>, request: &Req<Body>, length: u64 ) -> Params<'static> {

        let path = request.uri().path();
        let front = format!("/{}", call.script.index);
        let ( script, info ) = call.script.split(path).unwrap_or(( front.as_str(), "" ));
        let text = |value: &HeaderValue| String::from_utf8_lossy(value.as_bytes()).into_owned();
        let host = request.headers().get(HOST).map(text).unwrap_or_default();
        let name = host.rsplit_once(':').map(|( name, _ )| name).filter(|_| !host.starts_with('[')).unwrap_or(&host);

        let mut params = Params::default()
            .gateway_interface("CGI/1.1")
            .server_software("aegisx")
            .server_protocol("HTTP/1.1")
            .request_method(request.method().as_str().to_owned())
            .request_uri(request.uri().path_and_query().map_or("/", |target| target.as_str()).to_owned())
            .query_string(request.uri().query().unwrap_or("").to_owned())
            .document_root(call.script.root.to_string())
            .document_uri(script.to_owned())
            .script_name(script.to_owned())
            .script_filename(format!("{}{script}", call.script.root.trim_end_matches('/')))
            .remote_addr(call.peer.ip().to_string())
            .remote_port(call.peer.port())
            .server_name(name.to_owned())
            .server_port(call.port)
            .content_length(usize::try_from(length).unwrap_or(usize::MAX))
            .custom("REQUEST_SCHEME", if call.secure { "https" } else { "http" })
            .custom("PATH_INFO", info.to_owned())
            .custom("REDIRECT_STATUS", "200");

        if call.secure { params = params.custom("HTTPS", "on"); }

        if let Some(kind) = request.headers().get(CONTENT_TYPE) { params = params.content_type(text(kind)); }

        for ( name, value ) in request.headers().iter().filter(|( name, _ )| !matches!(name.as_str(), "content-type" | "content-length" | "proxy")) {

            let key: String = "HTTP_".chars().chain(name.as_str().chars().map(|letter| if letter == '-' { '_' } else { letter.to_ascii_uppercase() })).collect();

            params = params.custom(key, text(value));

        }

        params

    }

    fn head ( head: &[u8] ) -> AppResult<Option<( Res<Body>, usize )>> {

        let mut parsed = [httparse::EMPTY_HEADER; 96];

        let ( consumed, headers ) = match httparse::parse_headers(head, &mut parsed) {
            Ok(httparse::Status::Complete(done)) => done,
            Ok(httparse::Status::Partial) => return Ok(None),
            Err(_) => return Err(AppError::http(502, "fastcgi response head is malformed")),
        };

        let mut response = Res::new(Body::Empty);
        let mut status = None;

        for header in headers {

            if header.name.eq_ignore_ascii_case("status") {

                status = std::str::from_utf8(header.value).ok().and_then(|text| text.split_whitespace().next()).and_then(|code| code.parse::<u16>().ok());

                continue;

            }

            if let ( Ok(name), Ok(value) ) = ( HeaderName::from_bytes(header.name.as_bytes()), HeaderValue::from_bytes(header.value) ) { response.headers_mut().append(name, value); }

        }

        let fallback = if response.headers().contains_key(LOCATION) { 302 } else { 200 };

        *response.status_mut() = StatusCode::from_u16(status.unwrap_or(fallback)).map_err(|_| AppError::http(502, "fastcgi status is not a status code"))?;

        Ok(Some(( response, consumed )))

    }

}

impl Output {

    fn emit ( &mut self, chunk: Bytes ) -> Poll<Option<Result<Frame<Bytes>, AppError>>> {

        if let Some(probe) = &self.probe { probe.borrow_mut().feed(&chunk); }

        Poll::Ready(Some(Ok(Frame::data(chunk))))

    }

}

impl Frames for Output {

    fn frame ( &mut self, cx: &mut Context<'_> ) -> Poll<Option<Result<Frame<Bytes>, AppError>>> {

        if let Some(rest) = self.rest.take() { return self.emit(rest); }

        if self.done { return Poll::Ready(None); }

        match self.chunks.as_mut().poll_next(cx) {
            Poll::Ready(Some(Ok(chunk))) => self.emit(chunk),
            Poll::Ready(Some(Err(error))) => { self.done = true; Poll::Ready(Some(Err(error))) }
            Poll::Ready(None) => {

                self.done = true;

                if let Some(probe) = &self.probe { probe.borrow_mut().finish(); }

                Poll::Ready(None)

            }
            Poll::Pending => Poll::Pending,
        }

    }

    fn ended ( &self ) -> bool {

        self.done && self.rest.is_none()

    }

    fn guard ( &mut self, guard: Guard ) {

        self.guard = Some(guard);

    }

    fn probe ( &mut self, probe: Probe ) {

        self.probe = Some(probe);

    }

}
