use std::future::Future;
use std::time::Duration;

use tokio::runtime::{Builder, LocalOptions, LocalRuntime, Runtime};
use tokio::task::JoinHandle;

use crate::core::error::{AppError, AppFail, AppResult};
use super::arch::{Event, MAIN, RELOAD, Rt};

impl Rt {

    pub fn current_thread () -> AppResult<Runtime> {

        Builder::new_current_thread().enable_io().enable_time().build().or_fail("cannot build runtime")

    }

    pub fn local () -> AppResult<LocalRuntime> {

        Builder::new_current_thread().enable_io().enable_time().build_local(LocalOptions::default()).or_fail("cannot build runtime")

    }

    fn main () -> AppResult<&'static Runtime> {

        if let Some(runtime) = MAIN.get() { return Ok(runtime); }

        let runtime = Self::current_thread()?;
        let _ = MAIN.set(runtime);

        MAIN.get().or_fail("runtime not initialized")

    }

    pub fn block_on <F: Future> ( future: F ) -> AppResult<F::Output> {

        Ok(Self::main()?.block_on(future))

    }

    pub fn spawn_local <F> ( future: F ) -> JoinHandle<F::Output> where F: Future + 'static, F::Output: 'static {

        tokio::task::spawn_local(future)

    }

    pub async fn sleep ( millis: u64 ) {

        tokio::time::sleep(Duration::from_millis(millis)).await

    }

    pub async fn timeout <F: Future> ( what: &str, millis: u64, future: F ) -> AppResult<F::Output> {

        tokio::time::timeout(Duration::from_millis(millis), future).await.map_err(|_| AppError::timeout(what, millis))

    }

    pub fn reload () {

        RELOAD.notify_one();

    }

    pub async fn wait () -> AppResult<Event> {

        tokio::select! {
            event = Self::signals() => event,
            _ = RELOAD.notified() => Ok(Event::Reload),
        }

    }

    #[cfg(unix)]
    async fn signals () -> AppResult<Event> {

        use tokio::signal::unix::{SignalKind, signal};

        let mut term = signal(SignalKind::terminate()).or_fail("cannot listen for SIGTERM")?;
        let mut int = signal(SignalKind::interrupt()).or_fail("cannot listen for SIGINT")?;
        let mut hup = signal(SignalKind::hangup()).or_fail("cannot listen for SIGHUP")?;

        tokio::select! {
            _ = term.recv() => Ok(Event::Terminate),
            _ = int.recv() => Ok(Event::Terminate),
            _ = hup.recv() => Ok(Event::Reload),
        }

    }

    #[cfg(windows)]
    async fn signals () -> AppResult<Event> {

        use tokio::signal::windows::{ctrl_break, ctrl_c, ctrl_close, ctrl_shutdown};

        let mut interrupt = ctrl_c().or_fail("cannot listen for ctrl-c")?;
        let mut brk = ctrl_break().or_fail("cannot listen for ctrl-break")?;
        let mut close = ctrl_close().or_fail("cannot listen for close")?;
        let mut shutdown = ctrl_shutdown().or_fail("cannot listen for shutdown")?;

        tokio::select! {
            _ = interrupt.recv() => Ok(Event::Terminate),
            _ = brk.recv() => Ok(Event::Terminate),
            _ = close.recv() => Ok(Event::Terminate),
            _ = shutdown.recv() => Ok(Event::Terminate),
        }

    }

}
