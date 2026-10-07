use crate::core::error::AppError;

impl From<hyper::Error> for AppError {

    fn from ( error: hyper::Error ) -> Self {

        Self::Fail { message: "http transport".to_string(), source: Box::new(error) }

    }

}

impl From<http::Error> for AppError {

    fn from ( error: http::Error ) -> Self {

        Self::invalid("http message", error.to_string())

    }

}
