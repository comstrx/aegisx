use std::path::PathBuf;

use clap::Parser;

use crate::config::base::consts::TOOL;

#[derive(Parser)]
#[command(name = TOOL, version, about = "Local reverse proxy with observable request lifecycles", disable_help_subcommand = true)]
pub struct Cli {

    #[arg(long, value_name = "FILE", help = "Lua configuration file")]
    pub config: Option<PathBuf>,

    #[arg(long, help = "Validate the configuration and exit")]
    pub check: bool,

    #[arg(long, value_name = "ADDR", help = "Listen address, host:port")]
    pub listen: Option<String>,

    #[arg(long, value_name = "ADDR", help = "Upstream address, host:port")]
    pub upstream: Option<String>,

    #[arg(long, value_name = "N", help = "Worker threads; 0 uses every core")]
    pub workers: Option<usize>,

    #[arg(long, help = "Do not pin workers to cores")]
    pub no_pin: bool,

    #[arg(long, value_name = "LEVEL", help = "Log level filter")]
    pub log: Option<String>,

    #[arg(long, help = "Emit JSON logs")]
    pub json: bool,

}
