use http::header::HeaderName;
use crate::module::config::IdentityConfig;

/// Header names are validated and parsed once per immutable configuration.
pub struct Headers {
    pub request_id: HeaderName,
    pub forwarded_for: HeaderName,
    pub forwarded_proto: HeaderName,
}
impl Headers {
    pub fn new ( config: &IdentityConfig ) -> Self {
        let name=|value: &str| HeaderName::from_bytes(value.as_bytes()).expect("validated identity header");
        Self {request_id:name(&config.request_id_header),forwarded_for:name(&config.forwarded_for_header),
            forwarded_proto:name(&config.forwarded_proto_header)}
    }
}
