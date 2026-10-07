use std::time::Duration;
use reqwest::{Certificate, Client};
use crate::core::error::{AppFail, AppResult};
use crate::module::config::{BackendConfig, PoolOptions};

pub(super) struct Probe { client: Client, url: String, status: u16 }

impl Probe {

    pub fn new ( backend: &BackendConfig, options: &PoolOptions ) -> AppResult<Option<Self>> {

        let Some(path) = &options.health_path else { return Ok(None); };
        if options.health_interval_ms == 0 { return Ok(None); }
        let mut client = Client::builder().no_proxy().redirect(reqwest::redirect::Policy::none())
            .timeout(Duration::from_millis(options.health_timeout_ms)).pool_max_idle_per_host(1);
        let authority = if backend.tls {
            client = client.resolve(&backend.server_name, backend.address);
            if let Some(path) = &backend.ca_file {
                let bytes = std::fs::read(path).or_fail("Cannot read health probe CA")?;
                for certificate in Certificate::from_pem_bundle(&bytes).or_fail("Invalid health probe CA")? {
                    client = client.add_root_certificate(certificate);
                }
            }
            format!("https://{}:{}", backend.server_name, backend.address.port())
        } else { format!("http://{}", backend.address) };
        Ok(Some(Self {
            client: client.build().or_fail("Cannot configure HTTP health client")?,
            url: format!("{authority}{path}"), status: options.health_status,
        }))

    }

    pub async fn healthy ( &self ) -> bool {

        self.client.get(&self.url).send().await.is_ok_and(|response| response.status().as_u16() == self.status)

    }

}

impl super::Backend {
    pub async fn probe ( &self, timeout: Duration ) -> bool {
        match &self.probe {
            Some(probe) => probe.healthy().await,
            None => tokio::time::timeout(timeout, tokio::net::TcpStream::connect(self.config.address)).await.is_ok_and(|result| result.is_ok()),
        }
    }
}
