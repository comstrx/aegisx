use std::io::Read;
use std::path::Path;
use ort::{session::Session, value::TensorRef};
use sha2::{Digest, Sha256};

use crate::core::{domain::RiskScores, error::{AppError, AppFail, AppResult}};
use crate::module::config::Config;
use super::{Input, Model, ModelInfo};

impl Model {
    fn bundle ( directory: Option<&Path> ) -> AppResult<(Vec<u8>, ModelInfo)> {
        let (bytes, metadata) = if let Some(directory) = directory {
            let read = |name: &str, limit: u64| -> AppResult<Vec<u8>> {
                let mut bytes=Vec::new();
                std::fs::File::open(directory.join(name)).or_fail("Cannot open model artifact")?
                    .take(limit+1).read_to_end(&mut bytes).or_fail("Cannot read model artifact")?;
                if bytes.len() as u64>limit {return Err(AppError::invalid("Model artifact exceeds size limit"));}
                Ok(bytes)
            };
            (read("model.onnx",64*1024*1024)?,read("metadata.json",65536)?)
        } else {
            (include_bytes!("../../../../model/weights/model.onnx").to_vec(),
             include_bytes!("../../../../model/weights/metadata.json").to_vec())
        };
        let info:ModelInfo=serde_json::from_slice(&metadata).or_fail("Invalid model metadata")?;
        let count=if info.input_schema.as_deref()==Some(super::input::INPUT_SCHEMA) {
            let spec:serde_json::Value=serde_json::from_str(include_str!("../../../../model/src/aegisx_model/lifecycle.json"))
                .or_fail("Invalid lifecycle schema")?;
            if let Some(architectures)=spec["compatible_architectures"].as_array() {
                architectures.iter().find(|entry|entry["model_version"].as_str()==Some(info.model_version.as_str()))
                    .and_then(|entry|entry["parameter_count"].as_u64())
                    .ok_or_else(||AppError::invalid("Unsupported lifecycle architecture"))? as usize
            } else {
                spec["parameter_count"].as_u64().ok_or_else(||AppError::invalid("Missing lifecycle parameter count"))? as usize
            }
        } else if info.input_schema.is_none() {
            let architecture:serde_json::Value=serde_json::from_str(include_str!("../../../../model/src/aegisx_model/architecture.json"))
                .or_fail("Invalid numeric architecture")?;
            let layers=architecture["layers"].as_array().ok_or_else(||AppError::invalid("Missing numeric architecture"))?;
            layers.windows(2).map(|pair|(pair[0].as_u64().unwrap_or(0)+1)*pair[1].as_u64().unwrap_or(0)).sum::<u64>() as usize
        } else {return Err(AppError::invalid("Unsupported model input schema"));};
        if info.feature_version!=super::FEATURE_VERSION || info.parameter_count!=count
            || info.artifact_sha256!=crate::core::domain::hex(&Sha256::digest(&bytes)) {
            return Err(AppError::invalid("Model input schema, architecture or SHA256 does not match"));
        }
        Ok((bytes,info))
    }
    pub fn load () -> AppResult<Self> {Self::load_at(None)}
    pub fn load_at ( directory: Option<&Path> ) -> AppResult<Self> {
        let (bytes,info)=Self::bundle(directory)?;
        let session=Session::builder().or_fail("Cannot initialize inference")?
            .with_intra_threads(1).map_err(|error| AppError::invalid(error.to_string())).or_fail("Cannot set inference threads")?
            .with_inter_threads(1).map_err(|error| AppError::invalid(error.to_string())).or_fail("Cannot set inference threads")?
            .commit_from_memory(&bytes).or_fail("Cannot load model")?;
        let mut model=Self {session,info};
        model.predict(&Input::empty([0.0;super::FEATURE_COUNT]))?;
        Ok(model)
    }
    pub fn validate_policy ( config: &Config, info: Option<&ModelInfo> ) -> AppResult<()> {
        if config.needs_model() && info.is_none() {return Err(AppError::invalid("Model worker is required"));}
        if config.needs_enforcement() && !config.model.allow_unvalidated
            && !info.is_some_and(|info|info.deployment_ready && matches!(info.source.as_str(),"user_labeled"|"http_params"|"http_corpus"|"lifecycle_corpus")) {
            return Err(AppError::invalid("Enforcement requires an evaluated user-labeled artifact; use allow_unvalidated only for experiments"));
        }
        Ok(())
    }
    pub fn score ( &mut self, features: super::Vector ) -> AppResult<f32> {
        Ok(self.predict(&Input::empty(features))?.risk)
    }
    pub fn predict ( &mut self, input: &Input ) -> AppResult<RiskScores> {
        if !input.valid() {return Err(AppError::invalid("Invalid bounded model tensors"));}
        let features=TensorRef::from_array_view(([1,super::FEATURE_COUNT],input.features.as_slice())).or_fail("Cannot create numeric tensor")?;
        let text_model=self.info.input_schema.is_some();
        let outputs=if text_model {
            let text=TensorRef::from_array_view(([1,2,super::input::TEXT_BYTES],input.text.as_slice())).or_fail("Cannot create text tensor")?;
            let names=TensorRef::from_array_view(([1,super::input::EVENT_COUNT,super::input::EVENT_BYTES],input.event_text.as_slice())).or_fail("Cannot create event text tensor")?;
            let values=TensorRef::from_array_view(([1,super::input::EVENT_COUNT,super::input::EVENT_VALUES],input.event_values.as_slice())).or_fail("Cannot create event values tensor")?;
            let coverage=TensorRef::from_array_view(([1,4],input.coverage.as_slice())).or_fail("Cannot create coverage tensor")?;
            self.session.run(ort::inputs!["features"=>features,"text"=>text,"event_text"=>names,"event_values"=>values,"coverage"=>coverage])
        } else {self.session.run(ort::inputs!["features"=>features])}.or_fail("Inference failed")?;
        let read=|name:&str| -> AppResult<f32> {
            let output=outputs.get(name).ok_or_else(||AppError::invalid("Missing risk output"))?;
            let (shape,values)=output.try_extract_tensor::<f32>().or_fail("Invalid model output")?;
            if shape.as_ref()!=[1,1] || values.len()!=1 || !values[0].is_finite() || !(0.0..=1.0).contains(&values[0]) {
                return Err(AppError::invalid("Expected finite risk tensor [1,1] in [0,1]"));
            }
            Ok(values[0])
        };
        let risk=read("risk")?;
        let scores=RiskScores {risk,content:if text_model {read("content_risk")?} else {risk},
            journey:if text_model {read("journey_risk")?} else {0.0}};
        if text_model && (scores.risk-scores.content.max(scores.journey)).abs()>1e-6 {
            return Err(AppError::invalid("Inconsistent combined model score"));
        }
        Ok(scores)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn near_threshold_decisions_match_python () {
        let fixtures:Vec<serde_json::Value>=serde_json::from_str(include_str!("../../../../model/weights/boundary-parity.json")).unwrap();
        let mut model=Model::load().unwrap();
        for fixture in fixtures {
            let input=Input::from_json(&fixture["input"]).unwrap();
            let scores=model.predict(&input).unwrap();
            let actual=if fixture["task"]=="content" {scores.content} else {scores.journey};
            let expected=fixture["score"].as_f64().unwrap() as f32;
            let threshold=fixture["threshold"].as_f64().unwrap() as f32;
            assert_eq!(actual>=threshold,expected>=threshold,"{actual} != {expected}, threshold {threshold}");
        }
    }
    #[test]
    fn embedded_model_matches_python_reference () {
        let fixtures:Vec<serde_json::Value>=serde_json::from_str(include_str!("../../../../model/weights/parity.json")).unwrap();
        let mut model=Model::load().unwrap();
        for fixture in fixtures {
            let features:Vec<f32>=serde_json::from_value(fixture["features"].clone()).unwrap();
            let mut input=Input::empty(features.try_into().unwrap());
            if model.info.input_schema.is_some() {
                input.text=serde_json::from_value(fixture["text"].clone()).unwrap();
                input.event_text=serde_json::from_value(fixture["event_text"].clone()).unwrap();
                input.event_values=serde_json::from_value(fixture["event_values"].clone()).unwrap();
                input.coverage=serde_json::from_value(fixture["coverage"].clone()).unwrap();
            }
            let scores=model.predict(&input).unwrap();
            assert!((scores.risk-fixture["score"].as_f64().unwrap() as f32).abs()<1e-5);
            if model.info.input_schema.is_some() {
                assert!((scores.content-fixture["content_score"].as_f64().unwrap() as f32).abs()<1e-5);
                assert!((scores.journey-fixture["journey_score"].as_f64().unwrap() as f32).abs()<1e-5);
            }
        }
        assert!(model.score([f32::NAN;super::super::FEATURE_COUNT]).is_err());
        let mut bad=Input::empty([0.0;super::super::FEATURE_COUNT]);bad.text[0]=257;
        assert!(model.predict(&bad).is_err());
    }
}
