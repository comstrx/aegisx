use http::StatusCode;
use regex::Regex;

#[derive(Debug)]
pub struct Rewrite {
    pub(super) regex    : Regex,
    pub(super) to       : String,
    pub(super) redirect : Option<StatusCode>,
    pub(super) query    : bool,
}

#[derive(Debug, PartialEq, Eq)]
pub enum Outcome {
    Keep,
    Target(String),
    Redirect(String, StatusCode),
}
