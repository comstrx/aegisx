#[derive(serde::Deserialize)]
pub struct Envelope {
    pub cache_scores: bool,
    pub journal: bool,
    pub admission: [f32;16],
    pub sample: Vec<u8>,
    pub sample_seen: usize,
    pub response_sample: Vec<u8>,
    pub response_seen: usize,
    pub response_available: bool,
    pub events_truncated: bool,
    pub outcome: [f32;8],
    pub request_id: String,
    pub backend_events: Vec<crate::module::lifecycle::BackendEvent>,
}
impl Envelope {
    pub fn raw ( &self ) -> super::Vector {
        let mut raw = [0.0; super::FEATURE_COUNT];
        raw[..16].copy_from_slice(&self.admission);
        let content = super::content::extract(&self.sample, self.sample_seen);
        raw[16..32].copy_from_slice(&content[..16]);
        raw[32..40].copy_from_slice(&self.outcome);
        raw[40..].copy_from_slice(&content[16..]);
        raw
    }
}
