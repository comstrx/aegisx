#![forbid(unsafe_code)]

mod app;
mod core;
mod module;

use core::error::AppExitCode;

fn main () -> AppExitCode {

    match app::Cli::run() {
        Ok(()) => AppExitCode::SUCCESS,
        Err(error) => error.report(),
    }

}
