use pingora::proxy::{FailToProxy, Session};
use pingora::{Error, ErrorSource, ErrorType};

use super::arch::{Context, Proxy};

impl Proxy {

    pub(super) async fn failure ( &self, session: &mut Session, error: &Error, context: &Context ) -> FailToProxy {

        let code = match error.etype() {
            ErrorType::HTTPStatus(code) => *code,
            ErrorType::ConnectTimedout | ErrorType::ReadTimedout | ErrorType::WriteTimedout => {
                if error.esource() == &ErrorSource::Downstream { 408 } else { 504 }
            },
            _ => match error.esource() {
                ErrorSource::Upstream => 502,
                ErrorSource::Downstream => 400,
                _ => 500,
            },
        };
        let actual = if let Some(response) = session.response_written() {
            response.status.as_u16()
        } else if Self::respond(session, context, code).await.is_ok() {
            code
        } else {
            0
        };

        FailToProxy { error_code: actual, can_reuse_downstream: false }

    }

}
