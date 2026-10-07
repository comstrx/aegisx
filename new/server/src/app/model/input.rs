use serde_json::Value;

use crate::core::error::{AppError, AppResult};
use super::arch::{Architecture, Input};

impl Input {

    pub fn empty ( arch: &Architecture ) -> Self {

        Self {
            features     : vec![0.0; arch.feature_count],
            text         : vec![0; arch.text_streams * arch.text_bytes],
            event_text   : vec![0; arch.event_count * arch.event_bytes],
            event_values : vec![0.0; arch.event_count * arch.event_values],
            coverage     : vec![0.0; arch.coverage],
        }

    }

    pub fn from_json ( value: &Value, arch: &Architecture ) -> AppResult<Self> {

        let field = |name: &str| value.get(name).cloned().unwrap_or(Value::Null);

        let input = Self {
            features     : serde_json::from_value(field("features")).map_err(|error| AppError::invalid("model input", format!("features: {error}")))?,
            text         : serde_json::from_value(field("text")).map_err(|error| AppError::invalid("model input", format!("text: {error}")))?,
            event_text   : serde_json::from_value(field("event_text")).map_err(|error| AppError::invalid("model input", format!("event_text: {error}")))?,
            event_values : serde_json::from_value(field("event_values")).map_err(|error| AppError::invalid("model input", format!("event_values: {error}")))?,
            coverage     : serde_json::from_value(field("coverage")).map_err(|error| AppError::invalid("model input", format!("coverage: {error}")))?,
        };

        if !input.fits(arch) { return Err(AppError::invalid("model input", "tensors are out of bounds or mis-sized")); }

        Ok(input)

    }

    pub fn fits ( &self, arch: &Architecture ) -> bool {

        self.features.len() == arch.feature_count
            && self.text.len() == arch.text_streams * arch.text_bytes
            && self.event_text.len() == arch.event_count * arch.event_bytes
            && self.event_values.len() == arch.event_count * arch.event_values
            && self.coverage.len() == arch.coverage
            && self.features.iter().chain(&self.event_values).chain(&self.coverage).all(|value| value.is_finite() && (0.0..=1.0).contains(value))
            && self.text.iter().chain(&self.event_text).all(|value| (0..=256).contains(value))

    }

    pub fn write_text ( slot: &mut [i64], bytes: &[u8] ) {

        for ( value, byte ) in slot.iter_mut().zip(bytes) { *value = i64::from(*byte) + 1; }

    }

}
