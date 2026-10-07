use std::path::Path;

use memchr::memmem::Finder;
use serde::Deserialize;

use crate::core::error::{AppError, AppResult};
use crate::core::parse::Json;
use super::arch::{ADMISSION, CONTENT_FIXED, OUTCOME, PATTERN_GROUPS, Schema};

#[derive(Deserialize)]
struct Feature {
    name  : String,
    scale : f32,
}

#[derive(Deserialize)]
struct Lexical {
    buckets_per_width : usize,
}

#[derive(Deserialize)]
struct File {
    version          : u32,
    features         : Vec<Feature>,
    content_patterns : Vec<Vec<String>>,
    lexical          : Lexical,
}

const LEADING: [&str; ADMISSION] = [
    "read_method", "write_method", "path_length", "query_length", "header_count", "declared_body_bytes", "has_body", "prior_requests_10s",
    "prior_failures_10s", "prior_blocks_10s", "in_flight", "seconds_since_last", "actor_age_seconds", "previous_latency_ms", "path_depth", "has_query",
];

impl Schema {

    pub fn load ( path: &Path, version: u32, feature_count: usize ) -> AppResult<Self> {

        let bytes = std::fs::read(path).map_err(|error| AppError::config("add_model", format!("cannot read {}: {error}", path.display())))?;
        let file: File = Json::parse(&bytes).map_err(|error| AppError::config("add_model", format!("{}: {error}", path.display())))?;
        let buckets = file.lexical.buckets_per_width;
        let fail = |message: &str| AppError::config("add_model", format!("{}: {message}", path.display()));

        if file.version != version { return Err(fail(&format!("feature version {} does not match model feature version {version}", file.version))); }

        if !buckets.is_power_of_two() || file.features.len() != ADMISSION + CONTENT_FIXED + OUTCOME + 2 * buckets || file.features.len() != feature_count {

            return Err(fail("feature count does not match the lexical layout or the model architecture"));

        }

        if !file.features.iter().take(ADMISSION).map(|feature| feature.name.as_str()).eq(LEADING) { return Err(fail("admission features are not in the expected order")); }

        if file.features.iter().any(|feature| feature.name.is_empty() || !feature.scale.is_finite() || feature.scale <= 0.0) { return Err(fail("every feature needs a finite positive scale")); }

        if file.content_patterns.len() != PATTERN_GROUPS || file.content_patterns.iter().any(|group| group.is_empty() || group.iter().any(String::is_empty)) {

            return Err(fail(&format!("expected {PATTERN_GROUPS} non-empty content pattern groups")));

        }

        let patterns = file.content_patterns.iter().map(|group| group.iter().map(|pattern| Finder::new(pattern.as_bytes()).into_owned()).collect()).collect();

        Ok(Self {
            version,
            names    : file.features.iter().map(|feature| feature.name.clone()).collect(),
            scales   : file.features.iter().map(|feature| feature.scale).collect(),
            patterns,
            buckets,
        })

    }

    pub fn count ( &self ) -> usize {

        self.scales.len()

    }

    pub fn normalize ( &self, raw: &[f32] ) -> Option<Vec<f32>> {

        if raw.len() != self.scales.len() || raw.iter().any(|value| !value.is_finite() || *value < 0.0) { return None; }

        Some(raw.iter().zip(&self.scales).map(|( value, scale )| (value / scale).clamp(0.0, 1.0)).collect())

    }

}
