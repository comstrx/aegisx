use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};

use tokio::io::AsyncWriteExt;
use tokio::net::UdpSocket;

use crate::app::{Fence, Hint, Lease, Picker, State};
use crate::config::{Acl, StreamConfig};
use crate::core::error::{AppError, AppResult};
use crate::core::log::{debug, warn};
use crate::core::net::{Address, Preamble};
use crate::core::rt::Rt;
use crate::core::sync::{Local, Watch};
use crate::http::server::{Listener, Stream};
use crate::http::upstream::Tunnel;
use super::arch::{HELLO_MS, Relay};

const DATAGRAM: usize = 65_535;
const REPLY: usize = 16_384;

type Flows = Rc<RefCell<HashMap<SocketAddr, ( Rc<UdpSocket>, Rc<Cell<Instant>> )>>>;

impl Relay {

    pub fn new ( settings: Arc<StreamConfig>, state: State ) -> AppResult<Self> {

        let runtime = state.load();
        let pool = runtime.pools.find(&settings.upstream).map(|pool| pool.index).ok_or_else(|| AppError::config("add_stream", format!("stream `{}` points to unknown pool `{}`", settings.name, settings.upstream)))?;
        let picker = Local::new(Picker::new(&runtime.pools));
        let names = settings.sni.iter().filter_map(|( name, pool )| runtime.pools.find(pool).map(|pool| ( name.to_ascii_lowercase().into_boxed_str(), pool.index ))).collect();

        let fence = Fence::compile(&Acl::default(), &settings.acl);
        let quota = if settings.max_connections == 0 { 0 } else { settings.max_connections.div_ceil(runtime.snapshot.config.worker_count().max(1)) };

        Ok(Self { settings, state, picker, pool, names, fence, quota, active: Rc::new(Cell::new(0)) })

    }

    fn named ( &self, name: &str ) -> Option<usize> {

        self.names.iter().find(|( pattern, _ )| **pattern == *name).or_else(|| self.names.iter().find(|( pattern, _ )| pattern.strip_prefix('*').is_some_and(|suffix| name.len() > suffix.len() && name.ends_with(suffix)))).map(|( _, pool )| *pool)

    }

    pub async fn serve ( self, mut listener: Listener, mut stop: Watch ) {

        let relay = Rc::new(self);

        loop {

            let accepted = tokio::select! {
                accepted = listener.accept() => accepted,
                _ = stop.wait() => break,
            };

            let ( stream, peer ) = match accepted {
                Some(Ok(accepted)) => accepted,
                None => break,
                Some(Err(error)) => { warn!(%error, stream = %relay.settings.name, "stream accept failed"); Rt::sleep(10).await; continue; }
            };

            if relay.fence.as_ref().is_some_and(|fence| !fence.admits(peer.ip())) || relay.quota > 0 && relay.active.get() >= relay.quota {

                debug!(%peer, stream = %relay.settings.name, "stream connection refused by its access list or its connection limit");

                continue;

            }

            stream.tune();
            relay.active.set(relay.active.get() + 1);

            let relay = relay.clone();

            Rt::spawn_local(async move {

                if let Err(error) = relay.relay(stream, peer).await { debug!(%peer, %error, stream = %relay.settings.name, "stream session ended"); }

                relay.active.set(relay.active.get().saturating_sub(1));

            });

        }

    }

    pub async fn datagrams ( self, socket: std::net::UdpSocket, mut stop: Watch ) {

        let socket = match UdpSocket::from_std(socket) {
            Ok(socket) => Rc::new(socket),
            Err(error) => { warn!(%error, stream = %self.settings.name, "datagram socket is not usable"); return; }
        };

        let relay = Rc::new(self);
        let flows: Flows = Rc::new(RefCell::new(HashMap::new()));
        let mut buffer = vec![0u8; DATAGRAM];

        loop {

            let received = tokio::select! {
                received = socket.recv_from(&mut buffer) => received,
                _ = stop.wait() => break,
            };

            let ( size, peer ) = match received {
                Ok(received) => received,
                Err(error) => { debug!(%error, stream = %relay.settings.name, "datagram receive failed"); continue; }
            };

            if relay.fence.as_ref().is_some_and(|fence| !fence.admits(peer.ip())) { continue; }

            let known = flows.borrow().get(&peer).map(|( upstream, seen )| { seen.set(Instant::now()); upstream.clone() });

            let upstream = match known {
                Some(upstream) => upstream,
                None => match relay.flow(peer, &socket, &flows).await {
                    Ok(upstream) => upstream,
                    Err(error) => { debug!(%peer, %error, stream = %relay.settings.name, "datagram flow refused"); continue; }
                },
            };

            if let Err(error) = upstream.send(&buffer[..size]).await { debug!(%peer, %error, stream = %relay.settings.name, "datagram forward failed"); }

        }

    }

    async fn flow ( &self, peer: SocketAddr, socket: &Rc<UdpSocket>, flows: &Flows ) -> AppResult<Rc<UdpSocket>> {

        let runtime = self.state.load();
        let Some(pool) = runtime.pools.get(self.pool) else { return Err(AppError::message("stream pool vanished")); };
        let now = runtime.pools.now_ms();
        let headers = http::HeaderMap::new();
        let uri = http::Uri::default();
        let hint = Hint { ip: peer.ip(), secure: false, headers: &headers, uri: &uri };
        let Some(backend) = self.picker.with_mut(|picker| picker.pick(&runtime.pools, pool, &[], now, hint)).and_then(|index| pool.backends.get(index)) else { return Err(AppError::message("no stream backend available")); };
        let Address::Tcp(target) = &backend.addr else { return Err(AppError::unsupported("datagram backends need an ip address")); };
        let local: SocketAddr = if target.is_ipv4() { ( std::net::Ipv4Addr::UNSPECIFIED, 0 ).into() } else { ( std::net::Ipv6Addr::UNSPECIFIED, 0 ).into() };
        let upstream = Rc::new(UdpSocket::bind(local).await.map_err(|error| AppError::network(target.to_string(), error.to_string()))?);

        upstream.connect(target).await.map_err(|error| AppError::network(target.to_string(), error.to_string()))?;

        let seen = Rc::new(Cell::new(Instant::now()));
        let lease = pool.counting.then(|| Lease::new(backend.clone()));
        let idle = Duration::from_millis(self.settings.idle_ms.max(1));

        flows.borrow_mut().insert(peer, ( upstream.clone(), seen.clone() ));

        let ( replies, listener, flows ) = ( upstream.clone(), socket.clone(), flows.clone() );

        Rt::spawn_local(async move {

            let _lease = lease;
            let mut buffer = vec![0u8; REPLY];

            loop {

                match tokio::time::timeout(idle, replies.recv(&mut buffer)).await {
                    Ok(Ok(size)) => { seen.set(Instant::now()); if listener.send_to(&buffer[..size], peer).await.is_err() { break; } }
                    Ok(Err(_)) => break,
                    Err(_) if seen.get().elapsed() >= idle => break,
                    Err(_) => {}
                }

            }

            flows.borrow_mut().remove(&peer);

        });

        Ok(upstream)

    }

    async fn relay ( &self, client: Stream, peer: SocketAddr ) -> AppResult<()> {

        let runtime = self.state.load();

        let chosen = match self.names.is_empty() {
            true => self.pool,
            false => Rt::timeout("stream hello", HELLO_MS, client.server_name()).await.ok().flatten().and_then(|name| self.named(&name)).unwrap_or(self.pool),
        };

        let Some(pool) = runtime.pools.get(chosen) else { return Err(AppError::message("stream pool vanished")); };
        let now = runtime.pools.now_ms();
        let headers = http::HeaderMap::new();
        let uri = http::Uri::default();
        let hint = Hint { ip: peer.ip(), secure: false, headers: &headers, uri: &uri };
        let Some(backend) = self.picker.with_mut(|picker| picker.pick(&runtime.pools, pool, &[], now, hint)).and_then(|index| pool.backends.get(index)) else { return Err(AppError::message("no stream backend available")); };
        let lease = pool.counting.then(|| Lease::new(backend.clone()));
        let timeout_ms = runtime.snapshot.config.client.connect_timeout_ms;

        let mut upstream = match Stream::dial(&backend.addr, timeout_ms).await {
            Ok(upstream) => { backend.succeed(); upstream }
            Err(error) => { backend.fail(pool, now); return Err(error); }
        };

        if let Some(wire) = self.settings.proxy_protocol { upstream.write_all(&Preamble::write(wire, peer, self.settings.listen)).await.map_err(|error| AppError::network(backend.addr.to_string(), error.to_string()))?; }

        let outcome = Tunnel::join(client, upstream, self.settings.idle_ms).await.map(|_| ()).map_err(|error| AppError::network(backend.addr.to_string(), error.to_string()));

        drop(lease);

        outcome

    }

}
