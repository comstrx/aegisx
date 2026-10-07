use hyper::upgrade::{OnUpgrade, Upgraded};
use hyper_util::rt::{TokioIo, TokioTimer};

use crate::core::error::{AppError, AppResult};
use super::arch::{Io, LocalExec, Upgrading};

impl Io {

    pub fn wrap <T> ( stream: T ) -> TokioIo<T> {

        TokioIo::new(stream)

    }

    pub fn timer () -> TokioTimer {

        TokioTimer::new()

    }

    pub fn upgrading ( extensions: &mut http::Extensions ) -> Option<Upgrading> {

        extensions.remove::<OnUpgrade>().map(|pending| Upgrading { pending })

    }

}

impl <F> hyper::rt::Executor<F> for LocalExec
where F: std::future::Future + 'static {

    fn execute ( &self, future: F ) {

        crate::core::rt::Rt::spawn_local(future);

    }

}

impl Upgrading {

    pub async fn wait ( self ) -> AppResult<TokioIo<Upgraded>> {

        self.pending.await.map(TokioIo::new).map_err(|error| AppError::network("client upgrade", error.to_string()))

    }

}
