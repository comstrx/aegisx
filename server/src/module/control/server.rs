use std::sync::Arc;
use std::thread::JoinHandle;
use actix_web::{App, HttpServer, web};
use crate::core::error::{AppError, AppFail, AppResult};
use crate::module::{config::ControlConfig, services::Services};

pub struct Control;
pub struct ControlGuard { handle: actix_web::dev::ServerHandle, thread: JoinHandle<()> }
pub(super) struct State { pub services: Arc<Services>, pub token: String, pub backend_token: Option<String>, pub config: ControlConfig }

impl Control {

    pub fn token ( config: &ControlConfig ) -> AppResult<String> {
        let token = std::env::var(&config.token_env).map_err(|_| AppError::invalid("Control token environment variable is missing"))?;
        if !(32..=4096).contains(&token.len()) || token.bytes().any(|value| !value.is_ascii_graphic()) {
            return Err(AppError::invalid("Control token requires 32–4096 printable ASCII bytes"));
        }
        Ok(token)
    }

    pub fn start ( services: Arc<Services> ) -> AppResult<Option<ControlGuard>> {

        let config = services.current.load().config.control.clone();
        if !config.enabled { return Ok(None); }
        let listener = std::net::TcpListener::bind(config.listen).or_fail("Cannot bind local control listener")?;
        let backend_token = config.backend_token_env.as_ref().map(|name| {
            let token=std::env::var(name).map_err(|_|AppError::invalid("Missing backend token"))?;
            if token.len()<32 || token.len()>4096 || !token.bytes().all(|value|value.is_ascii_graphic()) { return Err(AppError::invalid("Invalid backend token")); }
            Ok(token)
        }).transpose()?;
        if backend_token.as_ref().is_some_and(|token| Self::token(&config).is_ok_and(|admin|admin==*token)) {
            return Err(AppError::invalid("Backend and admin credentials must differ"));
        }
        let state = web::Data::new(State { backend_token, token: Self::token(&config)?, config, services });
        let (ready, received) = std::sync::mpsc::sync_channel(1);
        let thread = std::thread::Builder::new().name("aegisx-control".into()).spawn(move || {
            actix_web::rt::System::new().block_on(async move {
                let server = HttpServer::new(move || App::new().app_data(state.clone())
                    .app_data(web::PayloadConfig::new(4096)).default_service(web::to(super::api::dispatch)))
                    .workers(1).max_connections(256).disable_signals().shutdown_timeout(2)
                    .listen(listener).expect("Pre-bound control listener").run();
                let _ = ready.send(server.handle());
                let _ = server.await;
            });
        }).or_fail("Cannot start control worker")?;
        let handle = received.recv().or_fail("Control worker did not initialize")?;

        Ok(Some(ControlGuard { handle, thread }))

    }

}

impl ControlGuard {
    pub fn finish ( self ) -> AppResult<()> {
        actix_web::rt::System::new().block_on(self.handle.stop(true));
        self.thread.join().map_err(|_| AppError::invalid("Control worker panicked"))?;
        Ok(())
    }
}
