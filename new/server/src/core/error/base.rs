use std::process::ExitCode;

use super::arch::AppError;

impl AppError {

    pub fn message ( message: impl Into<String> ) -> Self {

        Self::Message(message.into())

    }

    pub fn parse ( format: impl Into<String>, message: impl Into<String> ) -> Self {

        Self::Parse { format: format.into(), message: message.into() }

    }

    pub fn invalid ( what: impl Into<String>, message: impl Into<String> ) -> Self {

        Self::Invalid { what: what.into(), message: message.into() }

    }

    pub fn not_found ( what: impl Into<String> ) -> Self {

        Self::NotFound(what.into())

    }

    pub fn unsupported ( what: impl Into<String> ) -> Self {

        Self::Unsupported(what.into())

    }

    pub fn timeout ( what: impl Into<String>, millis: u64 ) -> Self {

        Self::Timeout { what: what.into(), millis }

    }

    pub fn network ( target: impl Into<String>, message: impl Into<String> ) -> Self {

        Self::Network { target: target.into(), message: message.into() }

    }

    pub fn config ( key: impl Into<String>, message: impl Into<String> ) -> Self {

        Self::Config { key: key.into(), message: message.into() }

    }

    pub fn http ( status: u16, message: impl Into<String> ) -> Self {

        Self::Http { status, message: message.into() }

    }

    pub fn status ( &self ) -> u16 {

        match self {
            Self::Http { status, .. } => *status,
            Self::Timeout { .. }      => 504,
            Self::Network { .. }      => 502,
            Self::Io(_)               => 502,
            Self::NotFound(_)         => 404,
            Self::Invalid { .. }      => 400,
            Self::Unsupported(_)      => 501,
            _                         => 500,
        }

    }

    pub fn exit_code ( &self ) -> ExitCode {

        ExitCode::from(match self {
            Self::Message(_)     => 1,
            Self::Fail { .. }    => 1,
            Self::Io(_)          => 2,
            Self::Parse { .. }   => 3,
            Self::Invalid { .. } => 4,
            Self::NotFound(_)    => 5,
            Self::Unsupported(_) => 6,
            Self::Timeout { .. } => 7,
            Self::Network { .. } => 8,
            Self::Config { .. }  => 9,
            Self::Http { .. }    => 10,
        })

    }

    pub fn report ( &self ) -> ExitCode {

        eprintln!("error: {self}");

        let mut source = std::error::Error::source(self);

        while let Some(cause) = source {

            eprintln!("cause: {cause}");
            source = cause.source();

        }

        self.exit_code()

    }

}
