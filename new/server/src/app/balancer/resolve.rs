use std::collections::HashMap;
use std::net::{SocketAddr, ToSocketAddrs};
use std::sync::Arc;

use hickory_resolver::TokioResolver;
use hickory_resolver::proto::rr::RData;

use crate::app::State;
use crate::config::Config;
use crate::core::error::{AppError, AppResult};
use crate::core::log::{info, warn};
use crate::core::rt::Rt;
use crate::core::sync::Watch;
use super::arch::{Pools, Resolved};

impl Pools {

    pub fn names ( config: &Config ) -> Vec<( Arc<str>, u16, bool )> {

        let mut names: Vec<( Arc<str>, u16, bool )> = Vec::new();

        for backend in config.pools.values().flat_map(|pool| pool.backends.iter()) {

            let wanted = match &backend.srv {
                Some(service) => Some(( service.as_str(), 0, true )),
                None => backend.address.name().map(|( host, port )| ( host, port, false )),
            };

            if let Some(( host, port, service )) = wanted && !names.iter().any(|( known, _, _ )| **known == *host) { names.push(( host.into(), port, service )); }

        }

        names

    }

    pub fn resolve_blocking ( config: &Config ) -> AppResult<Resolved> {

        let mut resolved = HashMap::new();

        for ( host, port, service ) in Self::names(config) {

            let addresses = match service {
                true => Self::service_blocking(&host)?,
                false => Self::distinct(( host.as_ref(), port ).to_socket_addrs().map_err(|error| AppError::config("add_upstream", format!("cannot resolve {host}: {error}")))?),
            };

            resolved.insert(host, addresses);

        }

        Ok(resolved)

    }

    pub async fn resolve_loop ( state: State, mut stop: Watch, every_ms: u64 ) {

        loop {

            tokio::select! {
                _ = Rt::sleep(every_ms) => {}
                _ = stop.wait() => break,
            }

            let runtime = state.load();
            let names = Self::names(&runtime.snapshot.config);
            let mut fresh: Resolved = HashMap::with_capacity(names.len());
            let mut changed = false;

            for ( host, port, service ) in names {

                let looked = match service {
                    true => Self::service(&host).await.map_err(|error| error.to_string()),
                    false => tokio::net::lookup_host(( host.as_ref(), port )).await.map(Self::distinct).map_err(|error| error.to_string()),
                };

                let addresses = match looked {
                    Ok(addresses) => addresses,
                    Err(error) => { warn!(%host, %error, "backend name lookup failed, keeping previous addresses"); continue; }
                };

                if addresses.is_empty() { warn!(%host, "backend name resolved to nothing, keeping previous addresses"); continue; }

                changed |= state.resolved(&host).as_deref() != Some(addresses.as_slice());
                fresh.insert(host, addresses);

            }

            if !changed { continue; }

            match state.refresh(fresh) {
                Ok(()) => info!("backend addresses refreshed from dns"),
                Err(error) => warn!(%error, "backend refresh rejected"),
            }

        }

    }

    async fn service ( name: &str ) -> AppResult<Vec<SocketAddr>> {

        let fail = |error: String| AppError::network(name, error);
        let resolver = TokioResolver::builder_tokio().map_err(|error| fail(error.to_string()))?.build().map_err(|error| fail(error.to_string()))?;
        let found = resolver.srv_lookup(name).await.map_err(|error| fail(error.to_string()))?;
        let records: Vec<_> = found.answers().iter().filter_map(|record| match &record.data { RData::SRV(service) => Some(service), _ => None }).collect();
        let first = records.iter().map(|service| service.priority).min().ok_or_else(|| fail("the name has no SRV records".to_string()))?;
        let mut targets = Vec::new();

        for service in records.into_iter().filter(|service| service.priority == first) {

            match resolver.lookup_ip(service.target.to_ascii()).await {
                Ok(addresses) => targets.extend(addresses.iter().map(|ip| SocketAddr::new(ip, service.port))),
                Err(error) => warn!(target = %service.target, %error, "an SRV target did not resolve"),
            }

        }

        Ok(Self::distinct(targets.into_iter()))

    }

    fn service_blocking ( name: &Arc<str> ) -> AppResult<Vec<SocketAddr>> {

        let wanted = name.clone();
        let lookup = move || tokio::runtime::Builder::new_current_thread().enable_all().build().map_err(AppError::from).and_then(|runtime| runtime.block_on(Self::service(&wanted)));
        let addresses = std::thread::spawn(lookup).join().map_err(|_| AppError::config("add_upstream", format!("the lookup of {name} did not finish")))?.map_err(|error| AppError::config("add_upstream", format!("cannot resolve {name}: {error}")))?;

        if addresses.is_empty() { return Err(AppError::config("add_upstream", format!("{name} has no reachable SRV target"))); }

        Ok(addresses)

    }

    fn distinct ( addresses: impl Iterator<Item = SocketAddr> ) -> Vec<SocketAddr> {

        let mut list: Vec<SocketAddr> = addresses.collect();

        list.sort_unstable();
        list.dedup();

        list

    }

}
