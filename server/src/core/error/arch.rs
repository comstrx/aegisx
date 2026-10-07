use thiserror::Error;

pub type AppResult<T> = Result<T, AppError>;
pub type AppExitCode = std::process::ExitCode;

#[derive(Debug, Error)]
pub enum AppError {

    #[error("{0}")]
    Invalid(String),

    #[error("{message}")]
    Context {
        message: String,
        #[source]
        source: Box<dyn std::error::Error + Send + Sync>,
    },

}

pub trait AppFail<T> {

    fn or_fail ( self, message: impl Into<String> ) -> AppResult<T>;

}
