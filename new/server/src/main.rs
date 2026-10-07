use aegisx::core::error::AppExitCode;
use mimalloc::MiMalloc;

#[global_allocator]
static GLOBAL: MiMalloc = MiMalloc;

fn main () -> AppExitCode {

    match aegisx::app::Cli::run() {
        Ok(()) => AppExitCode::SUCCESS,
        Err(error) => error.report(),
    }

}
