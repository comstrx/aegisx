use super::arch::{AppError, AppFail, AppResult};

impl <T, E> AppFail <T> for Result <T, E> where E: std::error::Error + Send + Sync + 'static {

    fn or_fail ( self, message: impl Into<String> ) -> AppResult<T> {

        self.map_err(|error| AppError::Fail { message: message.into(), source: Box::new(error) })

    }

    fn or_fail_with <S: Into<String>> ( self, message: impl FnOnce() -> S ) -> AppResult<T> {

        self.map_err(|error| AppError::Fail { message: message().into(), source: Box::new(error) })

    }

}

impl <T> AppFail <T> for Option <T> {

    fn or_fail ( self, message: impl Into<String> ) -> AppResult<T> {

        self.ok_or_else(|| AppError::Message(message.into()))

    }

    fn or_fail_with <S: Into<String>> ( self, message: impl FnOnce() -> S ) -> AppResult<T> {

        self.ok_or_else(|| AppError::Message(message().into()))

    }

}
