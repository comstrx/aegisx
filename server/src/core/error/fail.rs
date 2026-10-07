use super::{AppError, AppFail, AppResult};

impl<T, E> AppFail<T> for Result<T, E>
where E: std::error::Error + Send + Sync + 'static {

    fn or_fail ( self, message: impl Into<String> ) -> AppResult<T> {

        self.map_err(|source| AppError::Context { message: message.into(), source: Box::new(source) })

    }

}
