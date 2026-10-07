use std::future::Future;
use std::thread;

use crate::core::error::{AppError, AppFail, AppResult};
use super::arch::{Rt, Workers};

impl Rt {

    pub fn workers <F, Fut> ( count: usize, pin: bool, body: F ) -> AppResult<Workers>
    where F: Fn(usize) -> Fut + Send + Sync + Clone + 'static, Fut: Future<Output = AppResult<()>> + 'static {

        let cores = if pin { core_affinity::get_core_ids().unwrap_or_default() } else { Vec::new() };
        let mut handles = Vec::with_capacity(count);

        for index in 0..count {

            let body = body.clone();
            let core = cores.get(index % cores.len().max(1)).copied();

            let handle = thread::Builder::new().name(format!("aegisx-{index}")).spawn(move || {

                if let Some(core) = core { core_affinity::set_for_current(core); }

                Self::local()?.block_on(body(index))

            }).or_fail_with(|| format!("cannot spawn worker {index}"))?;

            handles.push(handle);

        }

        Ok(Workers { handles })

    }

}

impl Rt {

    pub fn thread <F, Fut> ( name: &str, body: F ) -> AppResult<Workers>
    where F: FnOnce() -> Fut + Send + 'static, Fut: Future<Output = AppResult<()>> + 'static {

        let handle = thread::Builder::new().name(name.to_string()).spawn(move || {

            Self::local()?.block_on(body())

        }).or_fail_with(|| format!("cannot spawn thread {name}"))?;

        Ok(Workers { handles: vec![handle] })

    }

}

impl Workers {

    pub fn len ( &self ) -> usize {

        self.handles.len()

    }

    pub fn is_empty ( &self ) -> bool {

        self.handles.is_empty()

    }

    pub fn join ( self ) -> AppResult<()> {

        let mut failure = None;

        for handle in self.handles {

            match handle.join() {
                Ok(Ok(())) => {}
                Ok(Err(error)) => { failure.get_or_insert(error); }
                Err(_) => { failure.get_or_insert(AppError::message("worker panicked")); }
            }

        }

        match failure {
            Some(error) => Err(error),
            None => Ok(()),
        }

    }

}
