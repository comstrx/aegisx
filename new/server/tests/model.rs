use std::path::PathBuf;

use aegisx::app::{Input, Model, Models};
use aegisx::config::{Config, ModelSpec};
use serde_json::Value;

fn weights () -> PathBuf {

    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../model/weights")

}

fn fixtures ( name: &str ) -> Vec<Value> {

    serde_json::from_slice(&std::fs::read(weights().join(name)).expect("fixture file")).expect("fixture json")

}

#[test]
fn embedded_model_matches_the_python_reference () {

    let model = Model::load("lifecycle", &weights(), None, 1).expect("model");

    assert_eq!(model.meta().model_version, "lifecycle-v9");
    assert_eq!(model.schema().count(), 296);
    assert_eq!(model.meta().parameter_count, 895_178);
    assert_eq!(model.sha256(), model.meta().artifact_sha256);

    for fixture in fixtures("parity.json") {

        let input = Input::from_json(&fixture, &model.meta().architecture).expect("input");
        let scores = model.predict(&input).expect("scores");

        assert!((scores.risk - fixture["score"].as_f64().expect("score") as f32).abs() < 1e-5, "{scores:?} vs {fixture}");
        assert!((scores.content - fixture["content_score"].as_f64().expect("content") as f32).abs() < 1e-5);
        assert!((scores.journey - fixture["journey_score"].as_f64().expect("journey") as f32).abs() < 1e-5);

    }

}

#[test]
fn near_threshold_decisions_match_python () {

    let model = Model::load("lifecycle", &weights(), None, 1).expect("model");

    for fixture in fixtures("boundary-parity.json") {

        let input = Input::from_json(&fixture["input"], &model.meta().architecture).expect("input");
        let scores = model.predict(&input).expect("scores");
        let actual = if fixture["task"] == "content" { scores.content } else { scores.journey };
        let expected = fixture["score"].as_f64().expect("score") as f32;
        let threshold = fixture["threshold"].as_f64().expect("threshold") as f32;

        assert_eq!(actual >= threshold, expected >= threshold, "{actual} vs {expected} at {threshold}");

    }

}

#[test]
fn invalid_tensors_and_tampered_artifacts_are_rejected () {

    let model = Model::load("lifecycle", &weights(), None, 1).expect("model");
    let mut input = model.empty();

    assert!(model.predict(&input).is_ok());

    input.features[0] = f32::NAN;

    assert!(model.predict(&input).is_err());

    let mut input = model.empty();

    input.text[0] = 257;

    assert!(model.predict(&input).is_err());

    let mut input = model.empty();

    input.coverage.push(0.0);

    assert!(model.predict(&input).is_err());

    let dir = std::env::temp_dir().join(format!("aegisx-model-{}", std::process::id()));

    std::fs::create_dir_all(&dir).expect("temp dir");
    std::fs::copy(weights().join("model.onnx"), dir.join("model.onnx")).expect("copy model");
    std::fs::copy(weights().join("features.json"), dir.join("features.json")).expect("copy features");

    let mut meta: Value = serde_json::from_slice(&std::fs::read(weights().join("metadata.json")).expect("metadata")).expect("json");

    meta["artifact_sha256"] = Value::String("00".repeat(32));
    std::fs::write(dir.join("metadata.json"), meta.to_string()).expect("write metadata");

    assert!(Model::load("tampered", &dir, None, 1).expect_err("sha mismatch").to_string().contains("sha256"));

    meta["artifact_sha256"] = Value::String(model.sha256().to_string());
    meta["input_schema"] = Value::String("other".to_string());
    std::fs::write(dir.join("metadata.json"), meta.to_string()).expect("write metadata");

    assert!(Model::load("tampered", &dir, None, 1).expect_err("schema").to_string().contains("schema"));

    let _ = std::fs::remove_dir_all(&dir);

}

#[test]
fn registry_serves_configured_models_by_name () {

    let mut config = Config::default();

    config.models.insert("lifecycle".to_string(), ModelSpec { dir: weights(), features: None, threads: 1 });

    Models::install(&config).expect("install");

    let model = Model::named("lifecycle").expect("named");

    assert_eq!(model.name(), "lifecycle");
    assert!(model.predict(&model.empty()).is_ok());
    assert!(Model::named("missing").expect_err("missing").to_string().contains("missing"));
    assert_eq!(Models::names(), vec![std::sync::Arc::<str>::from("lifecycle")]);

}
