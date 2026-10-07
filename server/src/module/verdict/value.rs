use serde::{Serialize, Deserialize};
use crate::core::{domain::hex, error::{AppError, AppResult}};
use crate::module::cache::Key;

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Verdict {
    pub key: String,
    pub actor: String,
    pub route: String,
    pub reason: String,
    pub source: String,
    pub expires_ms: u64,
    pub created_ms: u64,
    pub request_id: Option<String>,
}
impl Verdict {
    pub(super) fn validate ( &self, key: &str, expires_ms: i64 ) -> AppResult<()> {
        let hex_key = |text: &str| text.len() == 64 && text.bytes().all(|byte| byte.is_ascii_hexdigit());
        if self.key != key || !hex_key(key) || !hex_key(&self.actor)
            || expires_ms < 0 || self.expires_ms != expires_ms as u64
            || self.created_ms >= self.expires_ms || self.expires_ms > i64::MAX as u64
            || self.route.is_empty() || self.route.len() > 64
            || !self.route.bytes().all(|byte| byte.is_ascii_alphanumeric() || b"_-.".contains(&byte))
            || self.reason.is_empty() || self.reason.len() > 128 || self.reason.chars().any(char::is_control)
            || !matches!(self.source.as_str(), "operator" | "model" | "backend")
            || self.request_id.as_ref().is_some_and(|value| uuid::Uuid::parse_str(value).is_err())
        { return Err(AppError::invalid("Inconsistent verdict record")); }
        Ok(())
    }
    pub(super) fn validate_key ( &self, key: &Key ) -> AppResult<()> {
        self.validate(&hex(key), i64::try_from(self.expires_ms).map_err(|_| AppError::invalid("Invalid verdict expiry"))?)
    }
}
