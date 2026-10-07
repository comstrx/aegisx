use std::sync::Arc;

use serde_json::Value;

use crate::core::error::{AppError, AppResult};
use super::arch::{Assist, Input, Model, Scores, Assessment};

impl Model {

    pub fn assist ( name: &str ) -> AppResult<Assist> {

        Ok(Model::named(name)?.assistant())

    }

    pub fn assistant ( self: &Arc<Self> ) -> Assist {

        Assist { model: self.clone(), input: self.empty(), threshold: 0.5 }

    }

}

impl Assist {

    pub fn features ( mut self, values: &[f32] ) -> Self {

        for ( slot, value ) in self.input.features.iter_mut().zip(values) { *slot = value.clamp(0.0, 1.0); }

        self

    }

    pub fn feature ( mut self, name: &str, value: f32 ) -> Self {

        if let Some(index) = self.model.schema().names.iter().position(|known| known == name) && let Some(slot) = self.input.features.get_mut(index) { *slot = value.clamp(0.0, 1.0); }

        self

    }

    pub fn text ( mut self, stream: usize, bytes: &[u8] ) -> Self {

        let width = self.model.meta().architecture.text_bytes;

        if let Some(slot) = self.input.text.get_mut(stream * width..(stream + 1) * width) { Input::write_text(slot, bytes); }

        self

    }

    pub fn event ( mut self, index: usize, name: &[u8], values: &[f32] ) -> Self {

        let shape = &self.model.meta().architecture;

        if let Some(slot) = self.input.event_text.get_mut(index * shape.event_bytes..(index + 1) * shape.event_bytes) { Input::write_text(slot, name); }

        if let Some(slot) = self.input.event_values.get_mut(index * shape.event_values..(index + 1) * shape.event_values) {

            for ( target, value ) in slot.iter_mut().zip(values) { *target = value.clamp(0.0, 1.0); }

        }

        self

    }

    pub fn coverage ( mut self, values: &[f32] ) -> Self {

        for ( slot, value ) in self.input.coverage.iter_mut().zip(values) { *slot = value.clamp(0.0, 1.0); }

        self

    }

    pub fn json ( mut self, value: &Value ) -> AppResult<Self> {

        self.input = Input::from_json(value, &self.model.meta().architecture)?;

        Ok(self)

    }

    pub fn input ( mut self, input: Input ) -> AppResult<Self> {

        if !input.fits(&self.model.meta().architecture) { return Err(AppError::invalid("model input", "tensors are out of bounds or mis-sized")); }

        self.input = input;

        Ok(self)

    }

    pub fn threshold ( mut self, risk: f32 ) -> Self {

        self.threshold = risk.clamp(0.0, 1.0);

        self

    }

    pub fn model ( &self ) -> &Arc<Model> {

        &self.model

    }

    pub fn score ( &self ) -> AppResult<Scores> {

        self.model.predict(&self.input)

    }

    pub fn verdict ( &self ) -> AppResult<Assessment> {

        let scores = self.score()?;

        Ok(Assessment { scores, risky: scores.risk >= self.threshold, dominant: if scores.content >= scores.journey { "content" } else { "journey" }, threshold: self.threshold })

    }

    pub async fn detach ( self ) -> AppResult<Assessment> {

        tokio::task::spawn_blocking(move || self.verdict()).await.map_err(|error| AppError::message(format!("model task failed: {error}")))?

    }

}
