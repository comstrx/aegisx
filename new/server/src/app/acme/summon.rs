use std::path::Path;
use std::pin::Pin;
use std::sync::Arc;

use rustls::pki_types::CertificateDer;
use rustls::pki_types::pem::PemObject;

use crate::app::Tokens;
use crate::config::{AcmeConfig, Config, OnDemandConfig};
use crate::core::error::{AppError, AppResult};
use crate::core::log::{info, warn};
use crate::core::net::Address;
use crate::http::tls::{Challenges, Demand, Held, Minted, Obtain, Tls};
use crate::http::upstream::{Client, Protocol, Upstream};
use super::arch::{Acme, Summon};

const ASK_MS: u64 = 5_000;

impl Summon {

    pub fn new ( settings: AcmeConfig, plan: OnDemandConfig, demand: Arc<Demand>, challenges: Option<Challenges>, tokens: Option<Tokens>, config: &Config ) -> Self {

        let fixed: Arc<[String]> = settings.domains.iter().chain(config.tls.iter().flat_map(|tls| tls.certificates.iter().flat_map(|certificate| certificate.names.iter()))).map(|name| name.to_ascii_lowercase()).collect();

        Self { settings, plan, demand, challenges, tokens, client: config.client_settings(), fixed, gate: Arc::new(tokio::sync::Mutex::new(())), budget: Arc::new(std::sync::Mutex::new(( 0, 0 ))) }

    }

    pub fn restore ( settings: &AcmeConfig, held: &Held ) {

        let Ok(entries) = std::fs::read_dir(settings.cache_dir.join("demand")) else { return; };

        for entry in entries.flatten() {

            let name = entry.file_name().to_string_lossy().into_owned();

            match Self::load(&entry.path(), settings.renew_days) {
                Ok(minted) => { if let Ok(mut held) = held.write() { held.insert(name, minted); } }
                Err(error) => warn!(%error, %name, "acme: a stored on-demand certificate is unusable"),
            }

        }

    }

    pub(super) fn keep ( settings: &AcmeConfig, name: &str, chain: &str, key_pem: &str ) -> AppResult<Minted> {

        let dir = settings.cache_dir.join("demand").join(name);

        std::fs::create_dir_all(&dir).map_err(|error| AppError::message(format!("acme: cannot create {}: {error}", dir.display())))?;

        Acme::store(&dir.join("cert.pem"), &dir.join("key.pem"), chain, key_pem)?;

        Self::load(&dir, settings.renew_days)

    }

    fn load ( dir: &Path, renew_days: u64 ) -> AppResult<Minted> {

        let cert = dir.join("cert.pem");
        let identity = Tls::identity("on_demand", &cert, &dir.join("key.pem"), None)?;
        let leaf = CertificateDer::pem_file_iter(&cert).ok().and_then(|mut certs| certs.next()).and_then(Result::ok).ok_or_else(|| AppError::message(format!("{} holds no certificate", cert.display())))?;
        let ( _, parsed ) = x509_parser::parse_x509_certificate(leaf.as_ref()).map_err(|error| AppError::message(format!("{}: {error}", cert.display())))?;
        let expires = u64::try_from(parsed.validity().not_after.timestamp()).unwrap_or(0);

        Ok(Minted { identity: Arc::new(identity), renew: expires.saturating_sub(renew_days * 86_400) })

    }

    fn known ( &self, name: &str ) -> bool {

        self.demand.held(name).is_some() || self.fixed.iter().any(|fixed| match fixed.strip_prefix("*.") {
            Some(suffix) => name.strip_suffix(suffix).and_then(|head| head.strip_suffix('.')).is_some_and(|head| !head.is_empty() && !head.contains('.')),
            None => fixed == name,
        })

    }

    fn spend ( &self ) -> bool {

        let Ok(mut budget) = self.budget.lock() else { return false; };
        let hour = Demand::now() / 3_600;

        if budget.0 != hour { *budget = ( hour, 0 ); }

        if budget.1 >= self.plan.per_hour { return false; }

        budget.1 += 1;

        true

    }

    async fn asked ( &self, ask: &str, name: &str ) -> AppResult<bool> {

        let rest = ask.strip_prefix("http://").unwrap_or(ask);
        let ( authority, path ) = rest.split_once('/').map_or(( rest, String::from("/") ), |( authority, path )| ( authority, format!("/{path}") ));
        let address = Address::parse(authority)?;

        let address = match address.name() {
            Some(( host, port )) => Address::Tcp(tokio::net::lookup_host(( host, port )).await.map_err(|error| AppError::network(authority, error.to_string()))?.next().ok_or_else(|| AppError::network(authority, "the name has no address"))?),
            None => address,
        };

        let target = format!("{path}{}domain={name}", if path.contains('?') { '&' } else { '?' });
        let outcome = Client::new(self.client).probe(&Upstream::new(address, Protocol::Http1), Some(&target), ASK_MS).await?;

        Ok(outcome.is_some_and(|( status, _ )| (200..300).contains(&status)))

    }

    async fn summon ( &self, name: &str ) -> AppResult<bool> {

        if self.known(name) || !self.demand.permits(name) { return Ok(false); }

        let _turn = self.gate.lock().await;

        if self.demand.held(name).is_some() { return Ok(false); }

        if let Some(ask) = &self.plan.ask && !self.asked(ask, name).await? { return Err(AppError::message("the ask endpoint did not allow the name")); }

        if !self.spend() { return Err(AppError::message("the hourly issuance budget is spent")); }

        let ( chain, key_pem ) = Acme::obtain(&self.settings, &self.challenges, &self.tokens, &[name.to_string()]).await?;
        let minted = Self::keep(&self.settings, name, &chain, &key_pem)?;

        if !self.demand.install(name, minted.identity, minted.renew) { return Err(AppError::message("the on-demand certificate store is full")); }

        Ok(true)

    }

}

impl Obtain for Summon {

    fn obtain ( &self, name: String ) -> Pin<Box<dyn Future<Output = ()>>> {

        let this = self.clone();

        Box::pin(async move {

            match this.summon(&name).await {
                Ok(true) => info!(%name, "acme: on-demand certificate issued"),
                Ok(false) => {}
                Err(error) => warn!(%error, %name, "acme: on-demand certificate was not issued"),
            }

        })

    }

}
