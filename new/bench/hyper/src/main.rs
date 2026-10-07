use std::cell::RefCell;
use std::net::SocketAddr;
use std::rc::Rc;
use std::thread;
use std::time::Duration;

use http::header::{HOST, HeaderValue};
use hyper::body::Incoming;
use hyper::client::conn::http1::{Builder as Dialer, SendRequest};
use hyper::server::conn::http1::Builder as Acceptor;
use hyper::service::service_fn;
use hyper::{Request, Response};
use hyper_util::rt::TokioIo;
use mimalloc::MiMalloc;
use socket2::{Domain, Protocol, Socket, Type};
use tokio::net::{TcpListener, TcpStream};
use tokio::runtime::{Builder, LocalOptions};
use tokio::task::LocalSet;

#[global_allocator]
static GLOBAL: MiMalloc = MiMalloc;

type Failure = Box<dyn std::error::Error + Send + Sync>;

#[derive(Clone)]
struct Options {
    listen      : SocketAddr,
    upstream    : SocketAddr,
    workers     : usize,
    localset    : bool,
    writev      : Option<bool>,
    dial_writev : Option<bool>,
    max_headers : Option<usize>,
    interval    : Option<u32>,
    rebuild     : bool,
    timeout     : bool,
    pin         : bool,
    steer       : bool,
    auto        : bool,
    tuned       : bool,
}

#[derive(Clone, Copy)]
struct LocalExec;

impl <F: std::future::Future + 'static> hyper::rt::Executor<F> for LocalExec {

    fn execute ( &self, future: F ) {

        tokio::task::spawn_local(future);

    }

}

struct State {
    options   : Options,
    authority : HeaderValue,
    pool      : RefCell<Vec<SendRequest<Incoming>>>,
}

impl Options {

    fn parse () -> Result<Self, Failure> {

        let mut options = Self { listen: "127.0.0.1:8081".parse()?, upstream: "127.0.0.1:3000".parse()?, workers: 1, localset: false, writev: None, dial_writev: None, max_headers: None, interval: None, rebuild: false, timeout: false, pin: false, steer: false, auto: false, tuned: false };
        let mut args = std::env::args().skip(1);

        while let Some(flag) = args.next() {

            match flag.as_str() {
                "--listen" => options.listen = args.next().ok_or("--listen needs a value")?.parse()?,
                "--upstream" => options.upstream = args.next().ok_or("--upstream needs a value")?.parse()?,
                "--workers" => options.workers = args.next().ok_or("--workers needs a value")?.parse()?,
                "--writev" => options.writev = Some(args.next().ok_or("--writev needs on|off")? == "on"),
                "--dial-writev" => options.dial_writev = Some(args.next().ok_or("--dial-writev needs on|off")? == "on"),
                "--max-headers" => options.max_headers = Some(args.next().ok_or("--max-headers needs a value")?.parse()?),
                "--event-interval" => options.interval = Some(args.next().ok_or("--event-interval needs a value")?.parse()?),
                "--log" => { args.next(); }
                "--localset" => options.localset = true,
                "--rebuild" => options.rebuild = true,
                "--timeout" => options.timeout = true,
                "--pin" => options.pin = true,
                "--steer" => options.steer = true,
                "--auto" => options.auto = true,
                "--tuned" => options.tuned = true,
                _ => {}
            }

        }

        Ok(options)

    }

}

fn bind ( addr: SocketAddr, cpu: Option<usize> ) -> std::io::Result<std::net::TcpListener> {

    let socket = Socket::new(Domain::for_address(addr), Type::STREAM, Some(Protocol::TCP))?;

    socket.set_reuse_address(true)?;
    socket.set_reuse_port(true)?;
    socket.set_nonblocking(true)?;

    if let Some(cpu) = cpu { socket.set_cpu_affinity(cpu)?; }

    socket.bind(&addr.into())?;
    socket.listen(4096)?;

    Ok(socket.into())

}

async fn connect ( state: &State ) -> Result<SendRequest<Incoming>, Failure> {

    let stream = TcpStream::connect(state.options.upstream).await?;

    stream.set_nodelay(true)?;

    let mut dialer = Dialer::new();

    if let Some(writev) = state.options.dial_writev { dialer.writev(writev); }

    let ( sender, connection ) = dialer.handshake(TokioIo::new(stream)).await?;

    tokio::task::spawn_local(async move { let _ = connection.await; });

    Ok(sender)

}

fn rebuilt ( request: Request<Incoming>, authority: &HeaderValue ) -> Request<Incoming> {

    let ( parts, body ) = request.into_parts();
    let mut copy = Request::new(body);

    *copy.method_mut() = parts.method;
    *copy.uri_mut() = parts.uri;

    copy.headers_mut().reserve(parts.headers.len() + 6);
    copy.headers_mut().insert(HOST, authority.clone());

    for ( name, value ) in &parts.headers { if *name != HOST { copy.headers_mut().append(name.clone(), value.clone()); } }

    copy

}

async fn forward ( mut request: Request<Incoming>, state: Rc<State> ) -> Result<Response<Incoming>, Failure> {

    match state.options.rebuild {
        true => request = rebuilt(request, &state.authority),
        false => { request.headers_mut().insert(HOST, state.authority.clone()); }
    }

    let ready = { let mut pool = state.pool.borrow_mut(); pool.iter().rposition(SendRequest::is_ready).map(|index| pool.swap_remove(index)) };

    let mut sender = match ready {
        Some(sender) => sender,
        None => { state.pool.borrow_mut().retain(|sender| !sender.is_closed()); connect(&state).await? }
    };

    let response = match state.options.timeout {
        true => tokio::time::timeout(Duration::from_secs(10), sender.send_request(request)).await??,
        false => sender.send_request(request).await?,
    };

    state.pool.borrow_mut().push(sender);

    Ok(response)

}

async fn serve ( index: usize, options: Options ) -> Result<(), Failure> {

    let listener = TcpListener::from_std(bind(options.listen, options.steer.then_some(index))?)?;
    let state = Rc::new(State { authority: HeaderValue::from_str(&options.upstream.to_string())?, pool: RefCell::new(Vec::with_capacity(256)), options });
    let mut acceptor = Acceptor::new();
    let mut detector = hyper_util::server::conn::auto::Builder::new(LocalExec);

    if let Some(writev) = state.options.writev { acceptor.writev(writev); }

    if let Some(count) = state.options.max_headers { acceptor.max_headers(count); }

    if state.options.tuned {

        acceptor.keep_alive(true).max_buf_size(16_384).writev(true).pipeline_flush(true).timer(hyper_util::rt::TokioTimer::new()).header_read_timeout(None);
        detector.http1().keep_alive(true).max_buf_size(16_384).writev(true).pipeline_flush(true).timer(hyper_util::rt::TokioTimer::new()).header_read_timeout(None);
        detector.http2().max_concurrent_streams(256).initial_stream_window_size(1_048_576).initial_connection_window_size(4_194_304).max_frame_size(16_384).max_header_list_size(65_536).keep_alive_interval(Duration::from_secs(20)).keep_alive_timeout(Duration::from_secs(20)).timer(hyper_util::rt::TokioTimer::new());

    }

    let detector = Rc::new(detector);

    loop {

        let ( stream, _ ) = listener.accept().await?;

        stream.set_nodelay(true)?;

        let state = state.clone();

        match state.options.auto {
            true => {

                let detector = detector.clone();

                tokio::task::spawn_local(async move { let _ = detector.serve_connection_with_upgrades(TokioIo::new(stream), service_fn(move |request| forward(request, state.clone()))).await; });

            }
            false => {

                let connection = acceptor.serve_connection(TokioIo::new(stream), service_fn(move |request| forward(request, state.clone())));

                tokio::task::spawn_local(async move { let _ = connection.await; });

            }
        }

    }

}

fn work ( index: usize, options: Options ) -> Result<(), Failure> {

    if options.pin { core_affinity::set_for_current(core_affinity::CoreId { id: index }); }

    let mut builder = Builder::new_current_thread();

    builder.enable_io().enable_time();

    if let Some(interval) = options.interval { builder.event_interval(interval); }

    match options.localset {
        true => { let runtime = builder.build()?; LocalSet::new().block_on(&runtime, serve(index, options)) }
        false => builder.build_local(LocalOptions::default())?.block_on(serve(index, options)),
    }

}

fn main () -> Result<(), Failure> {

    let options = Options::parse()?;
    let workers: Vec<_> = (0..options.workers.max(1)).map(|index| { let options = options.clone(); thread::spawn(move || work(index, options)) }).collect();

    for worker in workers {

        if let Ok(Err(error)) = worker.join() { eprintln!("worker failed: {error}"); }

    }

    Ok(())

}
