use std::{path::PathBuf, sync::Arc};

use pingora::listeners::{ListenerConfig, L4BufferSettings, TcpSocketOptions};
use pingora::listeners::tls::TlsSettings;
use pingora::server::{RunArgs, Server, configuration::ServerConf};

use crate::core::error::{AppError, AppFail, AppResult};
use crate::module::{config::Config, control::Control, inference::Model, proxy::Proxy,
    runtime::{Manager, Snapshot}, services::Services, webhook::Webhooks};

pub struct App;

impl App {

    fn listener ( config: &Config ) -> AppResult<ListenerConfig> {

        let mut socket = TcpSocketOptions::default();
        socket.so_reuseport=Some(!config.runtime.work_stealing && config.runtime.threads>1);
        let mut listener = ListenerConfig::tcp(config.listen.to_string())
            .l4_buffer(L4BufferSettings::default().write(config.runtime.write_buffer_bytes))
            .tcp_socket_options(socket);
        if let Some(tls) = &config.tls {
            let cert = tls.cert.to_str().ok_or_else(|| AppError::invalid("Certificate path must be UTF-8"))?;
            let key = tls.key.to_str().ok_or_else(|| AppError::invalid("Key path must be UTF-8"))?;
            listener = listener.tls(TlsSettings::intermediate(cert, key).or_fail("Cannot configure TLS certificate and key")?);
        }
        Ok(listener)

    }

    pub fn check ( config: Config ) -> AppResult<()> {

        Self::listener(&config)?;
        if config.control.enabled { Control::token(&config.control)?; }
        Webhooks::validate_secrets(&config.webhooks)?;
        let model = config.needs_model().then(|| Model::load_at(config.model.directory.as_deref())).transpose()?;
        Model::validate_policy(&config, model.as_ref().map(|model| &model.info))?;
        Snapshot::build(config, None)?;
        Ok(())

    }

    pub fn run ( config: Config, path: PathBuf ) -> AppResult<()> {

        let workers=if config.runtime.work_stealing {1} else {config.runtime.threads};
        let listeners=(0..workers).map(|_|Self::listener(&config)).collect::<AppResult<Vec<_>>>()?;
        if config.control.enabled { Control::token(&config.control)?; }
        Webhooks::validate_secrets(&config.webhooks)?;
        let (services, guards) = Services::start(config.clone())?;
        let control = Control::start(services.clone())?;
        let manager = Manager::start(path, services.current.clone(), services.engine.model_info.clone(), services.started)?;
        let configuration = |threads| ServerConf {
            threads, work_stealing: config.runtime.work_stealing,
            listener_tasks_per_fd: config.runtime.accept_tasks,
            upstream_keepalive_pool_size: config.runtime.upstream_keepalive_capacity, max_retries: 3,
            grace_period_seconds: Some(1), graceful_shutdown_timeout_seconds: Some(5),
            ..ServerConf::default()
        };
        let mut server = Server::new_with_opt_and_conf(None, configuration(config.runtime.threads));
        server.bootstrap();
        // One connector/reactor per worker avoids cross-runtime socket handoffs.
        // Shared Services still owns policy, admission, storage and analysis once.
        for (index,listener) in listeners.into_iter().enumerate() {
            let threads=if config.runtime.work_stealing {config.runtime.threads} else {1};
            let conf=Arc::new(configuration(threads));
            let mut service=pingora::proxy::http_proxy_service_with_name(&conf,Proxy::new(services.clone()),&format!("aegisx-proxy-{index}"));
            service.threads=Some(threads);
            service.add_listener(listener);
            server.add_service(service);
        }
        tracing::info!(listen = %config.listen, pools = config.pools.len(), model = ?config.model.mode,
            config_version = %services.current.load().version, "AegisX starting");
        server.run(RunArgs::default());

        let manager_result = manager.finish();
        let control_result = control.map(|guard| guard.finish()).transpose();
        let guards_result = guards.finish();
        tracing::info!(dropped_events = services.store.dropped(), webhooks = %services.webhooks.stats(), "AegisX stopped");
        manager_result?;
        control_result?;
        guards_result?;
        Ok(())

    }

}
