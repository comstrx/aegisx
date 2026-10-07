use serde::Deserialize;

use crate::core::error::{AppError, AppFail, AppResult};
use crate::module::memory::Snapshot;

#[derive(Deserialize)]
struct Feature { name: String, scale: f32 }

#[derive(Deserialize)]
pub struct Features {
    version: u32,
    features: Vec<Feature>,
}

impl Features {

    pub fn load () -> AppResult<Self> {

        let spec: Self = serde_json::from_str(include_str!("../../../../model/src/aegisx_model/features.json"))
            .or_fail("Invalid embedded feature definition")?;
        let expected = ["read_method","write_method","path_length","query_length","header_count","declared_body_bytes","has_body","prior_requests_10s","prior_failures_10s","prior_blocks_10s","in_flight","seconds_since_last","actor_age_seconds","previous_latency_ms","path_depth","has_query"];
        if spec.version != super::FEATURE_VERSION || spec.features.len() != super::FEATURE_COUNT
            || !spec.features.iter().take(16).map(|feature| feature.name.as_str()).eq(expected)
            || spec.features.iter().any(|feature| feature.name.is_empty() || !feature.scale.is_finite() || feature.scale <= 0.0)
        {
            return Err(AppError::invalid("Unsupported feature definition"));
        }

        Ok(spec)

    }

    pub fn extract ( &self, request: &crate::core::domain::Facts, previous: Snapshot ) -> [f32; 16] {

        [
            f32::from(request.read),
            f32::from(request.write),
            request.path_length as f32, request.query_length as f32, request.header_count as f32, request.declared_body as f32,
            f32::from(request.has_body),
            previous.requests as f32, previous.failures as f32, previous.blocks as f32,
            previous.in_flight as f32, previous.gap_seconds, previous.age_seconds, previous.latency_ms as f32,
            request.path_depth as f32,
            f32::from(request.query_length > 0),
        ]

    }

    pub fn normalize ( &self, raw: super::Vector ) -> Option<super::Vector> {
        if raw.iter().any(|value| !value.is_finite() || *value < 0.0) { return None; }
        Some(std::array::from_fn(|index| (raw[index] / self.features[index].scale).clamp(0.0, 1.0)))
    }
}

