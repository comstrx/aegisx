use super::{AppError, AppExitCode};

impl AppError {

    pub fn invalid ( message: impl Into<String> ) -> Self {

        Self::Invalid(message.into())

    }

    pub fn report ( &self ) -> AppExitCode {

        eprintln!("aegisx: {self}");
        let mut cause = std::error::Error::source(self);

        while let Some(error) = cause {
            eprintln!("  caused by: {error}");
            cause = error.source();
        }

        AppExitCode::FAILURE

    }

}
