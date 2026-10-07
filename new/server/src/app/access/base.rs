use std::cell::RefCell;
use std::io::Write;
use std::path::Path;
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, TrySendError, sync_channel};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use file_rotate::compression::Compression;
use file_rotate::suffix::{AppendCount, AppendTimestamp, FileLimit};
use file_rotate::{ContentLimit, FileRotate, TimeFrequency};
use http::Request;

use crate::config::{AccessConfig, AccessFormat, Every};
use crate::core::error::{AppError, AppResult};
use crate::core::log::warn;
use crate::core::rt::Rt;
use crate::core::list::Few;
use crate::core::time::Clock;
use super::arch::{Sink, Access, Entry, Journal, Log, Pattern};

const QUEUE: usize = 1_024;
const FIELD_MAX: usize = 4_096;

impl Access {

    pub fn spawn ( config: &AccessConfig ) -> AppResult<Option<Arc<Self>>> {

        if config.path.as_os_str().is_empty() { return Ok(None); }

        let ( path, plan ) = ( config.path.clone(), config.clone() );

        if let Sink::File(file) = Sink::open(&path) { Sink::File(file).write(b"").map_err(|error| AppError::config("set_access_log", format!("cannot open {}: {error}", path.display())))?; }

        let pattern = (!config.pattern.is_empty()).then(|| Pattern::compile(&config.pattern)).transpose()?.map(Arc::new);

        let ( sender, receiver ) = sync_channel::<Vec<u8>>(QUEUE);
        let thread = std::thread::Builder::new().name("aegisx-access".into()).spawn(move || Self::drain(&plan, receiver)).map_err(AppError::from)?;

        Ok(Some(Arc::new(Self { pattern, sender: Mutex::new(Some(sender)), thread: Mutex::new(Some(thread)), dropped: AtomicU64::new(0) })))

    }

    pub fn log ( self: &Arc<Self>, config: &AccessConfig ) -> Option<Journal> {

        let sender = self.sender.lock().ok()?.clone()?;
        let capacity = config.buffer.min(1 << 20);

        Some(Rc::new(Log { access: self.clone(), sender, format: config.format, pattern: self.pattern.clone(), limit: config.buffer, flush_ms: config.flush_ms, floor: config.min_status, buffer: RefCell::new(Vec::with_capacity(capacity)), stamp: RefCell::new(( u64::MAX, [b' '; 26] )) }))

    }

    pub fn dropped ( &self ) -> u64 {

        self.dropped.load(Ordering::Relaxed)

    }

    pub fn close ( &self ) {

        if let Ok(mut slot) = self.sender.lock() { slot.take(); }

        if let Some(thread) = self.thread.lock().ok().and_then(|mut slot| slot.take()) { let _ = thread.join(); }

    }

    fn drain ( plan: &AccessConfig, receiver: Receiver<Vec<u8>> ) {

        let path = plan.path.as_path();
        let mut sink = Sink::open(path).rotated(plan);

        while let Ok(chunk) = receiver.recv() {

            if let Err(error) = sink.write(&chunk) { warn!(%error, path = %path.display(), "access log write failed"); }

        }

    }

}

impl Sink {

    pub fn open ( path: &Path ) -> Self {

        let text = path.to_string_lossy();

        match text.split_once("://") {
            _ if text == "stdout" => Self::Stdout,
            Some(( "syslog", target )) => Self::Datagram { target: target.to_string(), socket: None, syslog: true },
            Some(( "udp", target )) => Self::Datagram { target: target.to_string(), socket: None, syslog: false },
            Some(( "tcp", target )) => Self::Stream { target: target.to_string(), link: None },
            _ => Self::File(path.to_path_buf()),
        }

    }

    pub fn rotated ( self, plan: &AccessConfig ) -> Self {

        let Self::File(path) = &self else { return self; };
        let compression = if plan.rotate_compress { Compression::OnRotate(1) } else { Compression::None };

        let every = match plan.rotate_every {
            Every::Never => None,
            Every::Hourly => Some(TimeFrequency::Hourly),
            Every::Daily => Some(TimeFrequency::Daily),
            Every::Weekly => Some(TimeFrequency::Weekly),
            Every::Monthly => Some(TimeFrequency::Monthly),
        };

        match ( plan.rotate_bytes, every ) {
            ( 0, None ) => self,
            ( 0, Some(every) ) => Self::Rotating(Box::new(FileRotate::new(path, AppendTimestamp::default(FileLimit::MaxFiles(plan.rotate_keep)), ContentLimit::Time(every), compression, None))),
            ( bytes, _ ) => Self::Rotating(Box::new(FileRotate::new(path, AppendCount::new(plan.rotate_keep), ContentLimit::BytesSurpassed(usize::try_from(bytes).unwrap_or(usize::MAX)), compression, None))),
        }

    }

    pub fn write ( &mut self, bytes: &[u8] ) -> std::io::Result<()> {

        match self {
            Self::Stdout => std::io::stdout().lock().write_all(bytes),
            Self::File(path) => std::fs::OpenOptions::new().append(true).create(true).open(path)?.write_all(bytes),
            Self::Rotating(file) => file.write_all(bytes),
            Self::Datagram { target, socket, syslog } => {

                if socket.is_none() {

                    let fresh = std::net::UdpSocket::bind(( std::net::Ipv4Addr::UNSPECIFIED, 0 ))?;

                    fresh.connect(target.as_str())?;
                    *socket = Some(fresh);

                }

                let Some(link) = socket.as_ref() else { return Ok(()); };

                for line in bytes.split(|byte| *byte == b'\n').filter(|line| !line.is_empty()) {

                    let sent = match syslog {
                        true => link.send(&[b"<134>1 - - aegisx - - - ".as_slice(), line].concat()),
                        false => link.send(line),
                    };

                    if let Err(error) = sent { *socket = None; return Err(error); }

                }

                Ok(())

            }
            Self::Stream { target, link } => {

                if link.is_none() { *link = Some(std::net::TcpStream::connect(target.as_str())?); }

                let Some(stream) = link.as_mut() else { return Ok(()); };

                let outcome = stream.write_all(bytes);

                if outcome.is_err() { *link = None; }

                outcome

            }
        }

    }

}

impl Log {

    pub fn start ( log: &Journal ) {

        let log = log.clone();

        Rt::spawn_local(async move {

            loop {

                Rt::sleep(log.flush_ms).await;
                log.flush();

            }

        });

    }

    pub fn open <B> ( &self, request: &Request<B>, peer: std::net::SocketAddr, secure: bool, client: Option<&str> ) -> Entry {

        let target = request.uri().path_and_query().map_or("/", |target| target.as_str()).as_bytes();
        let referer = request.headers().get(http::header::REFERER).map_or(&b""[..], |value| value.as_bytes());
        let agent = request.headers().get(http::header::USER_AGENT).map_or(&b""[..], |value| value.as_bytes());
        let mut text = Vec::with_capacity(target.len() + referer.len() + agent.len() + 40);
        let mut marks = [0u16; 4];

        for ( slot, field ) in marks.iter_mut().zip([target, referer, agent]) {

            text.extend_from_slice(&field[..field.len().min(FIELD_MAX)]);
            *slot = text.len() as u16;

        }

        marks[3] = marks[2];

        let started = Instant::now();

        let mut entry = Entry { peer, secure, captured: Vec::new(), cuts: Few::new(), method: request.method().clone(), version: request.version(), status: 0, text, marks, route: None, backend: None, header_us: 0, attempts: 0, sent: None, received: 0, started, started_ms: Clock::wall_ms(started) };

        if let Some(pattern) = &self.pattern { pattern.capture(request.headers(), client, &mut entry); }

        entry

    }

    pub fn record ( &self, entry: &Entry ) {

        if entry.status < self.floor { return; }

        let mut buffer = self.buffer.borrow_mut();

        let stamp = self.stamp(entry.started_ms);

        match ( &self.pattern, self.format ) {
            ( Some(pattern), _ ) => Self::custom(&mut buffer, &stamp, entry, pattern),
            ( None, AccessFormat::Combined ) => Self::combined(&mut buffer, &stamp, entry),
            ( None, AccessFormat::Json ) => Self::json(&mut buffer, &stamp, entry),
        }

        if buffer.len() < self.limit { return; }

        let pending = std::mem::replace(&mut *buffer, Vec::with_capacity(self.limit.min(1 << 20)));

        drop(buffer);
        self.send(pending);

    }

    pub fn flush ( &self ) {

        let pending = std::mem::take(&mut *self.buffer.borrow_mut());

        if !pending.is_empty() { self.send(pending); }

    }

    fn send ( &self, pending: Vec<u8> ) {

        match self.sender.try_send(pending) {
            Ok(()) => {}
            Err(TrySendError::Full(chunk)) => {

                let lines = chunk.iter().filter(|byte| **byte == b'\n').count() as u64;
                let total = self.access.dropped.fetch_add(lines, Ordering::Relaxed) + lines;

                warn!(lines, total, "access log queue full, entries dropped");

            }
            Err(TrySendError::Disconnected(_)) => {}
        }

    }

}

impl Entry {

    pub fn identify ( &mut self, id: &[u8] ) {

        self.text.truncate(usize::from(self.marks[2]));
        self.text.extend_from_slice(&id[..id.len().min(FIELD_MAX)]);
        self.marks[3] = self.text.len() as u16;

    }

    pub fn target ( &self ) -> &[u8] {

        &self.text[..usize::from(self.marks[0])]

    }

    pub fn referer ( &self ) -> &[u8] {

        &self.text[usize::from(self.marks[0])..usize::from(self.marks[1])]

    }

    pub fn agent ( &self ) -> &[u8] {

        &self.text[usize::from(self.marks[1])..usize::from(self.marks[2])]

    }

    pub fn request_id ( &self ) -> &[u8] {

        &self.text[usize::from(self.marks[2])..usize::from(self.marks[3])]

    }

    pub fn sent ( &self ) -> usize {

        self.sent.as_ref().map_or(0, |probe| probe.borrow().seen())

    }

}

impl Drop for Log {

    fn drop ( &mut self ) {

        self.flush();

    }

}
