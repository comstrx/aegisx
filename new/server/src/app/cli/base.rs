use std::path::Path;

use clap::Parser;

use crate::app::Boot;
use crate::config::Config;
use crate::config::base::consts::{CONFIG_FILE, UPSTREAM};
use crate::core::error::AppResult;
use crate::core::net::{Addr, Address};
use super::arch::Cli;

impl Cli {

    pub fn run () -> AppResult<()> {

        let cli = Self::parse();
        let mut config = Self::base(cli.config.as_deref())?;

        if let Some(listen) = &cli.listen { config.listen = Addr::parse(listen)?; }

        if let Some(upstream) = &cli.upstream { config.set_upstream(Address::parse(upstream)?); }

        if let Some(workers) = cli.workers { config.runtime.workers = workers; }

        if let Some(level) = &cli.log { config.log.level = level.clone(); }

        if cli.no_pin { config.runtime.pin = false; }

        if cli.json { config.log.json = true; }

        if config.pools.is_empty() { config.set_upstream(Addr::parse(UPSTREAM)?); }

        config.validate()?;

        if cli.check {

            let runtime = Boot::check(&config)?;

            println!("configuration valid: {} pools, {} routes, {} workers", config.pools.len(), runtime.snapshot.routes.len(), config.worker_count());

            return Ok(());

        }

        Boot::run(config, cli.config.as_deref())

    }

    fn base ( path: Option<&Path> ) -> AppResult<Config> {

        match path {
            Some(path) => Config::load(path),
            None if Path::new(CONFIG_FILE).is_file() => Config::load(Path::new(CONFIG_FILE)),
            None => Ok(Config::default()),
        }

    }

}
