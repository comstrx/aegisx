use tracing_subscriber::EnvFilter;

use crate::core::error::{AppError, AppResult};
use super::arch::{Log, READY};

impl Log {

    pub fn init ( level: &str, json: bool ) -> AppResult<()> {

        if READY.get().is_some() { return Ok(()); }

        let filter = EnvFilter::try_new(level).map_err(|error| AppError::config("log.level", error.to_string()))?;
        let builder = tracing_subscriber::fmt().with_env_filter(filter).with_writer(std::io::stderr).with_target(false);

        let result = match json {
            true => builder.json().flatten_event(true).try_init(),
            false => builder.compact().try_init(),
        };

        result.map_err(|error| AppError::message(format!("cannot initialize logging: {error}")))?;

        let _ = READY.set(true);

        Ok(())

    }

    pub fn ready () -> bool {

        READY.get().copied().unwrap_or(false)

    }

}
