use std::net::SocketAddr;
use std::path::Path;
use std::sync::{Arc, Mutex};

use crate::app::{Access, Analyser, Control, Pools, Runtime, State, Telemetry};
use crate::config::Config;
use crate::core::error::{AppError, AppResult};
use crate::core::log::{Log, info, warn};
use crate::http::h3::Quic;
use crate::core::net::Socket;
use crate::http::server::Listener;
use crate::core::rt::{Event, Rt, Workers};
use crate::core::sync::Signal;
use crate::core::sys::Sys;
use super::arch::{Boot, Running, Worker};

impl Boot {

    pub fn run ( config: Config, path: Option<&Path> ) -> AppResult<()> {

        Log::init(&config.log.level, config.log.json)?;

        match Sys::files(u64::MAX) {
            Ok(limit) if limit < (config.limits.max_in_flight as u64).saturating_mul(2).saturating_add(64) => warn!(limit, max_in_flight = config.limits.max_in_flight, "open file limit is lower than two descriptors per request in flight"),
            Ok(_) => {}
            Err(error) => warn!(%error, "cannot raise the open file limit"),
        }

        let running = Self::start(config)?;

        info!(listen = %running.config.listen, pools = running.config.pools.len(), routes = running.config.routes.len(), workers = running.workers.len(), pin = running.config.runtime.pin, "aegisx started");

        Sys::notify("READY=1");

        loop {

            match Rt::block_on(Rt::wait())?? {
                Event::Terminate => break,
                Event::Reload => {

                    Sys::notify("RELOADING=1");
                    Self::reload(&running, path);
                    Sys::notify("READY=1");

                }
            }

        }

        Sys::notify("STOPPING=1");

        info!("aegisx stopping");

        running.stop()?;

        info!("aegisx stopped");

        Ok(())

    }

    fn reload ( running: &Running, path: Option<&Path> ) {

        let Some(path) = path else { warn!("reload requested but no configuration file is in use"); return; };

        match Config::load(path).and_then(|config| running.reload(config)) {
            Ok(version) => info!(version, path = %path.display(), "configuration reloaded"),
            Err(error) => warn!(%error, path = %path.display(), "configuration reload rejected; previous configuration stays active"),
        }

    }

    pub fn check ( config: &Config ) -> AppResult<Arc<Runtime>> {

        let state = State::new(config.clone())?;
        let runtime = state.load();

        let telemetry = Arc::new(Telemetry::new(config.worker_count(), &config.telemetry));
        let analyser = Analyser::start(config, telemetry.clone())?;

        Control::prepare(config, state, telemetry, analyser.clone())?;

        if let Some(analyser) = analyser { analyser.stop(); }

        for pool in &runtime.pools.list { info!(pool = %Pools::describe(pool), "pool"); }

        Ok(runtime)

    }

    pub fn start ( config: Config ) -> AppResult<Running> {

        let count = config.worker_count();
        let mut bound = Listener::bind(config.listen, config.runtime.backlog, count, config.runtime.accept)?;

        #[cfg(unix)]
        if let Some(path) = &config.listen_unix { Listener::bind_unix(&mut bound, path, config.runtime.backlog)?; }

        let listeners: Vec<Option<Listener>> = bound.into_iter().map(Some).collect();
        let listeners = Arc::new(Mutex::new(listeners));
        let mut streams: Vec<Vec<Option<Listener>>> = Vec::with_capacity(config.streams.len());

        let mut datagrams: Vec<Vec<Option<std::net::UdpSocket>>> = Vec::with_capacity(config.streams.len());

        for stream in &config.streams {

            match stream.udp {
                true => { streams.push(Vec::new()); datagrams.push((0..count).map(|_| Socket::datagram(stream.listen, true).map(Some)).collect::<AppResult<_>>()?); }
                false => { streams.push(Listener::bind(stream.listen, config.runtime.backlog, count, config.runtime.accept)?.into_iter().map(Some).collect()); datagrams.push(Vec::new()); }
            }

        }

        let streams = Arc::new(Mutex::new(streams));
        let datagrams = Arc::new(Mutex::new(datagrams));
        let mut extras: Vec<Vec<Option<Listener>>> = Vec::with_capacity(config.listeners.len());

        for extra in &config.listeners { extras.push(Listener::bind(extra.address, config.runtime.backlog, count, config.runtime.accept)?.into_iter().map(Some).collect()); }

        let extras = Arc::new(Mutex::new(extras));
        let quic: Vec<Option<std::net::UdpSocket>> = match config.quic_settings() { Some(settings) => Quic::bind(&settings, count)?.into_iter().map(Some).collect(), None => Vec::new() };
        let quic = Arc::new(Mutex::new(quic));
        let config = Arc::new(config);
        let state = State::new((*config).clone())?;
        let telemetry = Arc::new(Telemetry::new(count, &config.telemetry));
        let analyser = Analyser::start(&config, telemetry.clone())?;
        let access = Access::spawn(&config.access)?;
        let admin = Control::prepare(&config, state.clone(), telemetry.clone(), analyser.clone())?;
        let ( signal, stop ) = Signal::new();

        let workers = Rt::workers(count, config.runtime.pin, {

            let config = config.clone();
            let state = state.clone();
            let telemetry = telemetry.clone();
            let analyser = analyser.clone();
            let access = access.clone();
            let listeners = listeners.clone();
            let streams = streams.clone();
            let extras = extras.clone();
            let quic = quic.clone();
            let stop = stop.clone();

            move |index| Worker { index, config: config.clone(), state: state.clone(), telemetry: telemetry.clone(), analyser: analyser.clone(), access: access.clone(), listeners: listeners.clone(), streams: streams.clone(), datagrams: datagrams.clone(), extras: extras.clone(), quic: quic.clone(), stop: stop.clone() }.run()

        })?;

        let control = admin.map(|admin| Control::start(admin, config.server_settings(false), stop)).transpose()?;

        Ok(Running { config, state, telemetry, analyser, access, signal, workers, control })

    }

}

impl Running {

    pub fn pools ( &self ) -> Arc<Pools> {

        self.state.load().pools.clone()

    }

    pub fn addr ( &self ) -> SocketAddr {

        self.config.listen

    }

    pub fn state ( &self ) -> &State {

        &self.state

    }

    pub fn reload ( &self, config: Config ) -> AppResult<u64> {

        if config.listen != self.config.listen { return Err(AppError::config("set_listen", "the listen address cannot change on reload; restart instead")); }

        if config.listeners != self.config.listeners { return Err(AppError::config("add_listen", "listeners cannot change on reload; restart instead")); }

        if config.control != self.config.control { return Err(AppError::config("set_control", "control settings cannot change on reload; restart instead")); }

        if config.analysis != self.config.analysis { return Err(AppError::config("set_analysis", "analysis settings cannot change on reload; restart instead")); }

        if config.decisions != self.config.decisions { return Err(AppError::config("set_decisions", "decision store settings cannot change on reload; restart instead")); }

        if config.telemetry.recent != self.config.telemetry.recent || config.telemetry.journeys != self.config.telemetry.journeys {

            return Err(AppError::config("set_telemetry", "telemetry capacities cannot change on reload; restart instead"));

        }

        self.state.reload(config)

    }

    pub fn telemetry ( &self ) -> &Arc<Telemetry> {

        &self.telemetry

    }

    pub fn control_addr ( &self ) -> Option<SocketAddr> {

        self.control.as_ref().map(|_| self.config.control.listen)

    }

    pub fn stop ( self ) -> AppResult<()> {

        self.signal.fire();

        let workers = self.workers.join();
        let control = self.control.map(Workers::join).unwrap_or(Ok(()));

        if let Some(analyser) = &self.analyser { analyser.stop(); }

        if let Some(access) = &self.access { access.close(); }

        if let Some(decisions) = self.state.decisions() { decisions.stop(); }

        workers.and(control)

    }

}
