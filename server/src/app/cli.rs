use std::{path::PathBuf, io::Read};

use clap::{Parser, Subcommand};

use crate::core::error::{AppError, AppFail, AppResult};
use crate::module::config::Config;
use crate::module::inference::{Input, Model};
use crate::module::storage::Store;
use super::run::App;

#[derive(Parser)]
#[command(name = "aegisx", version, about = "Local reverse proxy with observable request lifecycles")]
pub struct Cli {
    #[arg(long, default_value = "Aegisx.lua")]
    config: PathBuf,
    #[arg(long, help = "Validate configuration without starting the proxy")]
    check: bool,
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// Read local lifecycle events while the proxy is running.
    Inspect {
        #[arg(long)]
        database: PathBuf,
        #[arg(long)]
        request: Option<String>,
        #[arg(long, default_value_t = 100)]
        limit: usize,
    },
    /// Run the embedded model against bounded normalized JSON tensors.
    Score {
        #[arg(long, required_unless_present="input", conflicts_with="input")]
        features: Option<String>,
        #[arg(long, help="Path to a flat byte-journey-v1 tensor JSON object")]
        input: Option<PathBuf>,
    },
}

impl Cli {

    pub fn run () -> AppResult<()> {

        let cli = Self::parse();
        match cli.command {
            Some(Command::Inspect { database, request, limit }) => {
                let events = Store::inspect(&database, request.as_deref(), limit)?;
                println!("{}", serde_json::to_string_pretty(&events).or_fail("Cannot encode events")?);
                Ok(())
            }
            Some(Command::Score { features, input }) => {
                let mut model = Model::load()?;
                if let Some(path) = input {
                    let mut bytes=Vec::new();
                    std::fs::File::open(path).or_fail("Cannot open input tensors")?
                        .take(262145).read_to_end(&mut bytes).or_fail("Cannot read input tensors")?;
                    if bytes.len()>262144 {return Err(AppError::invalid("Input exceeds 256 KiB"));}
                    let value=serde_json::from_slice(&bytes).or_fail("Expected tensor JSON")?;
                    let score=model.predict(&Input::from_json(&value)?)?;
                    println!("{}",serde_json::to_string(&score).or_fail("Cannot encode scores")?);
                    return Ok(());
                }
                if model.info.input_schema.is_some() {
                    return Err(AppError::invalid("This model needs text/journey tensors: use score --input file.json"));
                }
                let values: Vec<f32> = serde_json::from_str(&features.unwrap_or_default()).or_fail("Expected schema-sized JSON values")?;
                let values: crate::module::inference::Vector = values.try_into().map_err(|_|AppError::invalid("Expected schema-sized JSON values"))?;
                println!("{}", Model::load()?.score(values)?);
                Ok(())
            }
            None => {
                let path = cli.config.canonicalize().or_fail("Cannot resolve configuration path")?;
                let config = Config::load(&path)?;
                if cli.check {
                    App::check(config)?;
                    println!("Configuration valid");
                    return Ok(());
                }
                tracing_subscriber::fmt()
                    .with_env_filter("warn,aegisx=info,pingora_proxy=off")
                    .json()
                    .with_writer(std::io::stderr)
                    .try_init()
                    .map_err(|error| AppError::invalid(format!("Cannot initialize logging: {error}")))?;
                App::run(config, path)
            }
        }

    }

}
