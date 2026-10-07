use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use super::{Envelope, Vector};

include!(concat!(env!("OUT_DIR"), "/lifecycle_schema.rs"));

pub struct Input {
    pub features: Vector,
    pub text: Vec<i64>,
    pub event_text: Vec<i64>,
    pub event_values: Vec<f32>,
    pub coverage: [f32; 4],
}
impl Input {
    pub fn from_json ( value: &Value ) -> crate::core::error::AppResult<Self> {
        use crate::core::error::{AppError, AppFail};
        let features: Vec<f32> = serde_json::from_value(value["features"].clone()).or_fail("Invalid numeric features")?;
        let input = Self {
            features: features.try_into().map_err(|_|AppError::invalid("Expected 296 numeric features"))?,
            text: serde_json::from_value(value["text"].clone()).or_fail("Invalid text tokens")?,
            event_text: serde_json::from_value(value["event_text"].clone()).or_fail("Invalid event tokens")?,
            event_values: serde_json::from_value(value["event_values"].clone()).or_fail("Invalid event values")?,
            coverage: serde_json::from_value(value["coverage"].clone()).or_fail("Invalid coverage")?,
        };
        if !input.valid() { return Err(AppError::invalid("Invalid bounded model tensors")); }
        Ok(input)
    }

    pub fn empty ( features: Vector ) -> Self {
        Self { features, text:vec![0;2*TEXT_BYTES], event_text:vec![0;EVENT_COUNT*EVENT_BYTES],
            event_values:vec![0.0;EVENT_COUNT*EVENT_VALUES], coverage:[0.0;4] }
    }
    pub fn from_envelope ( features: Vector, envelope: &Envelope ) -> Self {
        let mut input=Self::empty(features);
        Self::tokens(&envelope.sample,&mut input.text[..TEXT_BYTES]);
        Self::tokens(&envelope.response_sample,&mut input.text[TEXT_BYTES..]);
        for (index,event) in envelope.backend_events.iter().take(EVENT_COUNT).enumerate() {
            Self::tokens(format!("{}\n{}",event.service,event.operation).as_bytes(),
                &mut input.event_text[index*EVENT_BYTES..(index+1)*EVENT_BYTES]);
            let parent=event.parent_id.as_ref().and_then(|parent|
                envelope.backend_events[..index].iter().position(|prior|prior.span_id.as_ref()==Some(parent)));
            input.event_values[index*EVENT_VALUES..(index+1)*EVENT_VALUES].copy_from_slice(&[
                1.0,f32::from(event.state=="started"),f32::from(event.state=="completed"),f32::from(event.state=="failed"),
                Self::time(event.duration_ms.unwrap_or(0)),Self::time(event.elapsed_ms.unwrap_or(0)),
                parent.map_or(0.0,|parent|(parent+1) as f32/EVENT_COUNT as f32),f32::from(parent.is_some())]);
        }
        input.coverage=[f32::from(envelope.sample_seen>TEXT_BYTES || envelope.sample_seen>envelope.sample.len()),
            f32::from(envelope.response_seen>TEXT_BYTES || envelope.response_seen>envelope.response_sample.len()),
            f32::from(envelope.events_truncated || envelope.backend_events.len()>EVENT_COUNT),
            f32::from(envelope.response_available)];
        input
    }
    fn tokens ( bytes: &[u8], output: &mut [i64] ) {
        for (value,byte) in output.iter_mut().zip(bytes) { *value=i64::from(*byte)+1; }
    }
    fn time ( value: u64 ) -> f32 { ((value.min(86400000) as f64).ln_1p()/86400000_f64.ln_1p()) as f32 }
    pub fn valid ( &self ) -> bool {
        self.text.len()==2*TEXT_BYTES && self.event_text.len()==EVENT_COUNT*EVENT_BYTES
            && self.event_values.len()==EVENT_COUNT*EVENT_VALUES
            && self.features.iter().chain(&self.event_values).chain(&self.coverage).all(|value|value.is_finite() && (0.0..=1.0).contains(value))
            && self.text.iter().chain(&self.event_text).all(|value|(0..=256).contains(value))
    }
    pub fn key ( &self, artifact: &str ) -> [u8;32] {
        let mut hash=Sha256::new();hash.update(artifact.as_bytes());hash.update(INPUT_SCHEMA.as_bytes());
        // Batch fixed-width encodings: identical keys, fewer hasher calls and no heap scratch.
        let mut buffer=[0u8;512];
        let mut used=0;
        for value in self.features.iter().chain(&self.event_values).chain(&self.coverage) {
            buffer[used..used+4].copy_from_slice(&value.to_bits().to_le_bytes());used+=4;
            if used==buffer.len() {hash.update(buffer);used=0;}
        }
        hash.update(&buffer[..used]);used=0;
        for value in self.text.iter().chain(&self.event_text) {
            buffer[used..used+8].copy_from_slice(&value.to_le_bytes());used+=8;
            if used==buffer.len() {hash.update(buffer);used=0;}
        }
        hash.update(&buffer[..used]);
        hash.finalize().into()
    }
    pub fn summary ( &self ) -> Value {
        json!({"schema":INPUT_SCHEMA,"request_bytes":self.text[..TEXT_BYTES].iter().filter(|v|**v!=0).count(),
            "response_bytes":self.text[TEXT_BYTES..].iter().filter(|v|**v!=0).count(),
            "backend_events":self.event_values.as_chunks::<EVENT_VALUES>().0.iter().filter(|row|row[0]>0.0).count(),
            "request_truncated":self.coverage[0]>0.0,"response_truncated":self.coverage[1]>0.0,
            "events_truncated":self.coverage[2]>0.0,"response_available":self.coverage[3]>0.0,
            "raw_content_persisted":false,"recovery":"unavailable_without_volatile_text"})
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn lifecycle_encoding_matches_python_and_cache_keys_cover_every_modality () {
        let fixtures:Vec<Value>=serde_json::from_str(include_str!("../../../../model/weights/lifecycle-parity.json")).unwrap();
        let normalizer=super::super::Features::load().unwrap();
        for fixture in fixtures {
            let envelope:Envelope=serde_json::from_value(fixture["envelope"].clone()).unwrap();
            let features=normalizer.normalize(envelope.raw()).unwrap();
            let mut actual=Input::from_envelope(features,&envelope);
            let expected=Input::from_json(&fixture["expected"]).unwrap();
            assert_eq!(actual.text,expected.text);
            assert_eq!(actual.event_text,expected.event_text);
            assert_eq!(actual.coverage,expected.coverage);
            for (a,b) in actual.features.iter().chain(&actual.event_values).zip(expected.features.iter().chain(&expected.event_values)) {
                assert!((a-b).abs()<1e-6,"{a} != {b}");
            }
            let key=actual.key("model-a");
            assert_ne!(key,actual.key("model-b"));
            actual.text[TEXT_BYTES]=256;
            assert_ne!(key,actual.key("model-a"));
            let key=actual.key("model-a");
            actual.event_text[0]=256;
            assert_ne!(key,actual.key("model-a"));
            let key=actual.key("model-a");
            actual.event_values[7]=1.0-actual.event_values[7];
            assert_ne!(key,actual.key("model-a"));
        }
    }
    #[test]
    fn batched_cache_hash_preserves_the_complete_tensor_encoding () {
        let mut input=Input::empty([0.25;super::super::FEATURE_COUNT]);
        for (index,value) in input.text.iter_mut().enumerate() {*value=(index%257) as i64;}
        for (index,value) in input.event_text.iter_mut().enumerate() {*value=(index%251) as i64;}
        input.event_values.fill(0.5);input.coverage=[1.0,0.0,1.0,1.0];
        let mut reference=Sha256::new();
        reference.update(b"artifact");reference.update(INPUT_SCHEMA.as_bytes());
        for value in input.features.iter().chain(&input.event_values).chain(&input.coverage) {reference.update(value.to_bits().to_le_bytes());}
        for value in input.text.iter().chain(&input.event_text) {reference.update(value.to_le_bytes());}
        assert_eq!(input.key("artifact"),<[u8;32]>::from(reference.finalize()));
    }

}
