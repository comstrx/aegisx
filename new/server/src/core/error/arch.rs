use thiserror::Error;

pub type AppExitCode   = std::process::ExitCode;
pub type AppResult <T> = Result<T, AppError>;

pub trait AppFail <T> {

    fn or_fail ( self, message: impl Into<String> ) -> AppResult<T>;

    fn or_fail_with <S: Into<String>> ( self, message: impl FnOnce() -> S ) -> AppResult<T>;

}

#[derive(Debug, Error)]
pub enum AppError {

    #[error("{0}")]
    Message(String),

    #[error("{message}")]
    Fail { message: String, #[source] source: Box<dyn std::error::Error + Send + Sync> },

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("{format} parse error: {message}")]
    Parse { format: String, message: String },

    #[error("invalid {what}: {message}")]
    Invalid { what: String, message: String },

    #[error("not found: {0}")]
    NotFound(String),

    #[error("unsupported: {0}")]
    Unsupported(String),

    #[error("{what} timed out after {millis}ms")]
    Timeout { what: String, millis: u64 },

    #[error("network error ({target}): {message}")]
    Network { target: String, message: String },

    #[error("config `{key}`: {message}")]
    Config { key: String, message: String },

    #[error("http {status}: {message}")]
    Http { status: u16, message: String },

}
