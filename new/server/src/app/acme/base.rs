use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use instant_acme::{Account, AccountCredentials, AuthorizationStatus, ChallengeType, Identifier, LetsEncrypt, NewAccount, NewOrder, OrderStatus, RetryPolicy};
use rustls::pki_types::CertificateDer;
use rustls::pki_types::pem::PemObject;

use crate::app::State;
use crate::config::{AcmeChallenge, AcmeConfig};
use crate::core::error::{AppError, AppResult};
use crate::core::log::{info, warn};
use crate::core::rt::Rt;
use crate::core::sync::Watch;
use crate::http::tls::{Challenges, Demand, Tls};
use super::arch::{Acme, Summon};
use crate::app::Tokens;

const PLACEHOLDER: &str = "AegisX placeholder";
const RETRY_MIN_MS: u64 = 60_000;
const RETRY_MAX_MS: u64 = 3_600_000;
const CHECK_MS: u64 = 6 * 3_600_000;

impl Acme {

    pub fn new ( settings: AcmeConfig, state: State ) -> Self {

        Self { settings, state }

    }

    pub fn paths ( settings: &AcmeConfig ) -> ( PathBuf, PathBuf ) {

        ( settings.cache_dir.join("cert.pem"), settings.cache_dir.join("key.pem") )

    }

    pub fn materialize ( settings: &AcmeConfig ) -> AppResult<()> {

        std::fs::create_dir_all(&settings.cache_dir).map_err(|error| AppError::config("set_tls", format!("cannot create acme cache_dir {}: {error}", settings.cache_dir.display())))?;

        let ( cert, key ) = Self::paths(settings);

        if cert.is_file() && key.is_file() { return Ok(()); }

        let ( cert_pem, key_pem ) = Tls::self_signed(&settings.domains, PLACEHOLDER)?;

        Self::store(&cert, &key, &cert_pem, &key_pem)?;
        info!(dir = %settings.cache_dir.display(), "acme: placeholder certificate installed until the first order completes");

        Ok(())

    }

    pub fn due ( settings: &AcmeConfig ) -> bool {

        let ( cert, _ ) = Self::paths(settings);
        let Some(Ok(leaf)) = CertificateDer::pem_file_iter(&cert).ok().and_then(|mut certs| certs.next()) else { return true; };
        let Ok(( _, parsed )) = x509_parser::parse_x509_certificate(leaf.as_ref()) else { return true; };

        if parsed.issuer().to_string().contains(PLACEHOLDER) { return true; }

        let now = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |since| since.as_secs() as i64);

        parsed.validity().not_after.timestamp() - now < (settings.renew_days * 86_400) as i64

    }

    pub async fn run ( self, mut stop: Watch ) {

        let mut backoff = RETRY_MIN_MS;

        loop {

            self.renew().await;

            let wait = if !self.settings.domains.is_empty() && Self::due(&self.settings) {

                match self.order().await {
                    Ok(()) => { backoff = RETRY_MIN_MS; info!(domains = ?self.settings.domains, "acme: certificate issued"); CHECK_MS }
                    Err(error) => { warn!(%error, retry_ms = backoff, "acme: order failed"); let wait = backoff; backoff = (backoff * 2).min(RETRY_MAX_MS); wait }
                }

            } else { CHECK_MS };

            tokio::select! {
                _ = Rt::sleep(wait) => {}
                _ = stop.wait() => break,
            }

        }

    }

    async fn order ( &self ) -> AppResult<()> {

        let ( chain, key_pem ) = Self::obtain(&self.settings, &self.state.challenges(), &self.state.tokens(), &self.settings.domains).await?;
        let ( cert, key ) = Self::paths(&self.settings);

        Self::store(&cert, &key, &chain, &key_pem)?;
        self.state.reissue()

    }

    async fn renew ( &self ) {

        let held = self.state.held();
        let due: Vec<String> = held.read().map_or_else(|_| Vec::new(), |held| held.iter().filter(|( _, minted )| minted.renew <= Demand::now()).map(|( name, _ )| name.clone()).collect());

        for name in due {

            let renewed = async {

                let ( chain, key_pem ) = Self::obtain(&self.settings, &self.state.challenges(), &self.state.tokens(), std::slice::from_ref(&name)).await?;

                Summon::keep(&self.settings, &name, &chain, &key_pem)

            };

            match renewed.await {
                Ok(minted) => { if let Ok(mut held) = held.write() { held.insert(name.clone(), minted); } info!(%name, "acme: on-demand certificate renewed"); }
                Err(error) => warn!(%error, %name, "acme: on-demand renewal failed"),
            }

        }

    }

    pub(super) async fn obtain ( settings: &AcmeConfig, challenges: &Option<Challenges>, tokens: &Option<Tokens>, names: &[String] ) -> AppResult<( String, String )> {

        let account = Self::account(settings).await?;
        let identifiers: Vec<Identifier> = names.iter().map(|domain| Identifier::Dns(domain.clone())).collect();
        let mut order = account.new_order(&NewOrder::new(&identifiers)).await.map_err(Self::fail)?;
        let mut placed: Vec<String> = Vec::new();

        {

            let mut authorizations = order.authorizations();

            while let Some(result) = authorizations.next().await {

                let mut authorization = result.map_err(Self::fail)?;

                if authorization.status == AuthorizationStatus::Valid { continue; }

                match settings.challenge {
                    AcmeChallenge::TlsAlpn01 => {

                        let mut challenge = authorization.challenge(ChallengeType::TlsAlpn01).ok_or_else(|| AppError::message("acme: server offers no tls-alpn-01 challenge"))?;
                        let name = challenge.identifier().to_string();
                        let certified = Tls::challenge(&name, challenge.key_authorization().digest().as_ref())?;

                        if let Some(map) = challenges && let Ok(mut map) = map.write() { map.insert(name.clone(), certified); }

                        placed.push(name);
                        challenge.set_ready().await.map_err(Self::fail)?;

                    }
                    AcmeChallenge::Http01 => {

                        let mut challenge = authorization.challenge(ChallengeType::Http01).ok_or_else(|| AppError::message("acme: server offers no http-01 challenge"))?;
                        let token = challenge.token.clone();

                        if let Some(map) = tokens && let Ok(mut map) = map.write() { map.insert(token.clone(), challenge.key_authorization().as_str().to_owned()); }

                        placed.push(token);
                        challenge.set_ready().await.map_err(Self::fail)?;

                    }
                    AcmeChallenge::Dns01 => {

                        let mut challenge = authorization.challenge(ChallengeType::Dns01).ok_or_else(|| AppError::message("acme: server offers no dns-01 challenge"))?;
                        let record = format!("_acme-challenge.{}", challenge.identifier().to_string().trim_start_matches("*."));
                        let value = challenge.key_authorization().dns_value();

                        Self::hook(settings, "add", &record, &value).await?;
                        placed.push(format!("{record} {value}"));

                        Rt::sleep(settings.dns_wait_ms).await;

                        challenge.set_ready().await.map_err(Self::fail)?;

                    }
                }

            }

        }

        let status = order.poll_ready(&RetryPolicy::default()).await.map_err(Self::fail);

        Self::cleanup(settings, challenges, tokens, &placed);

        if status? != OrderStatus::Ready { return Err(AppError::message("acme: order did not become ready")); }

        let ( csr, key_pem ) = Tls::csr(names)?;

        order.finalize_csr(&csr).await.map_err(Self::fail)?;

        let chain = order.poll_certificate(&RetryPolicy::default()).await.map_err(Self::fail)?;

        Ok(( chain, key_pem ))

    }

    async fn account ( settings: &AcmeConfig ) -> AppResult<Account> {

        let path = settings.cache_dir.join("account.json");
        let builder = match &settings.directory_ca { Some(ca) => Account::builder_with_root(ca), None => Account::builder() }.map_err(Self::fail)?;

        if let Ok(text) = std::fs::read(&path) && let Ok(credentials) = serde_json::from_slice::<AccountCredentials>(&text) { return builder.from_credentials(credentials).await.map_err(Self::fail); }

        let contact: Vec<String> = settings.email.iter().map(|email| format!("mailto:{email}")).collect();
        let contact: Vec<&str> = contact.iter().map(String::as_str).collect();
        let ( account, credentials ) = builder.create(&NewAccount { contact: &contact, terms_of_service_agreed: true, only_return_existing: false }, Self::directory(settings), None).await.map_err(Self::fail)?;

        if let Ok(json) = serde_json::to_vec(&credentials) && let Err(error) = Self::write(&path, &json, true) { warn!(%error, "acme: cannot persist account credentials"); }

        Ok(account)

    }

    fn directory ( settings: &AcmeConfig ) -> String {

        match settings.directory.as_str() {
            "staging" => LetsEncrypt::Staging.url().to_owned(),
            "production" => LetsEncrypt::Production.url().to_owned(),
            url => url.to_owned(),
        }

    }

    async fn hook ( settings: &AcmeConfig, action: &'static str, record: &str, value: &str ) -> AppResult<()> {

        let Some(hook) = settings.dns_hook.clone() else { return Err(AppError::message("acme: dns_01 needs dns_hook")); };
        let ( record, value ) = ( record.to_owned(), value.to_owned() );

        let status = tokio::task::spawn_blocking(move || std::process::Command::new(&hook).args([action, record.as_str(), value.as_str()]).status()).await
            .map_err(|error| AppError::message(format!("acme: dns hook did not run: {error}")))?
            .map_err(|error| AppError::message(format!("acme: dns hook did not start: {error}")))?;

        if !status.success() { return Err(AppError::message(format!("acme: dns hook {action} exited with {status}"))); }

        Ok(())

    }

    fn cleanup ( settings: &AcmeConfig, challenges: &Option<Challenges>, tokens: &Option<Tokens>, placed: &[String] ) {

        if let Some(hook) = settings.dns_hook.clone().filter(|_| settings.challenge == AcmeChallenge::Dns01) {

            for entry in placed.iter().filter_map(|entry| entry.split_once(' ')).map(|( record, value )| ( record.to_owned(), value.to_owned() )) {

                let hook = hook.clone();

                tokio::task::spawn_blocking(move || std::process::Command::new(&hook).args(["remove", entry.0.as_str(), entry.1.as_str()]).status());

            }

            return;

        }

        if let Some(map) = challenges && let Ok(mut map) = map.write() { for name in placed { map.remove(name); } }

        if let Some(map) = tokens && let Ok(mut map) = map.write() { for token in placed { map.remove(token); } }

    }

    pub(super) fn store ( cert: &Path, key: &Path, cert_pem: &str, key_pem: &str ) -> AppResult<()> {

        Self::write(key, key_pem.as_bytes(), true)?;
        Self::write(cert, cert_pem.as_bytes(), false)

    }

    fn write ( path: &Path, bytes: &[u8], private: bool ) -> AppResult<()> {

        let temporary = path.with_extension("tmp");

        std::fs::write(&temporary, bytes).map_err(|error| AppError::message(format!("acme: cannot write {}: {error}", temporary.display())))?;

        #[cfg(unix)]
        if private {

            use std::os::unix::fs::PermissionsExt;

            let _ = std::fs::set_permissions(&temporary, std::fs::Permissions::from_mode(0o600));

        }

        #[cfg(not(unix))]
        let _ = private;

        std::fs::rename(&temporary, path).map_err(|error| AppError::message(format!("acme: cannot install {}: {error}", path.display())))

    }

    fn fail ( error: instant_acme::Error ) -> AppError {

        AppError::message(format!("acme: {error}"))

    }

}
