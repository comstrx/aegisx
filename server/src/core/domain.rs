use serde::{Serialize, Deserialize};

pub type Actor = [u8; 32];

#[derive(Default)]
pub struct Facts {
    pub read: bool,
    pub write: bool,
    pub path_length: usize,
    pub query_length: usize,
    pub header_count: usize,
    pub declared_body: u64,
    pub has_body: bool,
    pub path_depth: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Block {
    pub config_version: String,
    #[serde(default)]
    pub request_id: Option<String>,
    pub route: String,
    pub actor: String,
    pub ttl_ms: u64,
    pub reason: String,
}

pub fn hex ( bytes: &[u8] ) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut value = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        value.push(DIGITS[(byte >> 4) as usize] as char);
        value.push(DIGITS[(byte & 15) as usize] as char);
    }
    value
}

#[cfg(test)]
mod tests {
    #[test]
    fn actor_encoding_matches_the_contract () {
        let bytes: Vec<u8> = (0..=255).collect();
        let expected: String = bytes.iter().map(|byte| format!("{byte:02x}")).collect();
        assert_eq!(super::hex(&bytes), expected);
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct RiskScores { pub risk: f32, pub content: f32, pub journey: f32 }
