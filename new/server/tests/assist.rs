use std::path::PathBuf;
use std::sync::Arc;

use aegisx::app::Model;
use serde_json::Value;

fn weights () -> PathBuf {

    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../model/weights")

}

#[test]
fn the_assistant_scores_like_predict_and_judges_against_a_threshold () {

    let model = Arc::new(Model::load("lifecycle", &weights(), None, 1).expect("model"));
    let fixtures: Vec<Value> = serde_json::from_slice(&std::fs::read(weights().join("parity.json")).expect("fixtures")).expect("json");
    let input = fixtures.first().expect("one fixture");

    let direct = model.predict(&aegisx::app::Input::from_json(input, &model.meta().architecture).expect("input")).expect("predict");
    let assessment = model.assistant().json(input).expect("assist input").threshold(0.5).verdict().expect("verdict");

    assert_eq!(assessment.scores, direct);
    assert_eq!(assessment.risky, direct.risk >= 0.5);
    assert!(matches!(assessment.dominant, "content" | "journey"));

    let blank = model.assistant().feature("nonexistent-feature", 0.9).text(0, b"GET /login").coverage(&[1.0]).verdict().expect("blank verdict");

    assert!(blank.scores.risk.is_finite());

}

#[tokio::test]
async fn detached_scoring_runs_off_the_async_thread () {

    let model = Arc::new(Model::load("lifecycle", &weights(), None, 1).expect("model"));
    let assessment = model.assistant().threshold(0.9).detach().await.expect("detached verdict");

    assert!((0.0..=1.0).contains(&assessment.scores.risk));
    assert_eq!(assessment.threshold, 0.9);

}
