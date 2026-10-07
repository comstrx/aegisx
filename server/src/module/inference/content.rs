use std::sync::LazyLock;
use super::{CONTENT_COUNT, LEXICAL_BUCKETS};
use memchr::memmem::Finder;

static PATTERNS: LazyLock<Vec<Vec<Finder<'static>>>> = LazyLock::new(|| {
    serde_json::from_str::<serde_json::Value>(include_str!("../../../../model/src/aegisx_model/features.json"))
        .expect("embedded schema")["content_patterns"].as_array().expect("patterns").iter()
        .map(|items| items.as_array().unwrap().iter().map(|value| Finder::new(value.as_str().unwrap()).into_owned()).collect()).collect()
});
fn decoded ( input: &[u8] ) -> std::borrow::Cow<'_,[u8]> {
    if memchr::memchr2(b'%',b'+',input).is_none() && !input.iter().any(u8::is_ascii_uppercase) {
        return std::borrow::Cow::Borrowed(input);
    }
    let mut text=input.to_vec();
    // Decoding only shrinks the buffer; both passes reuse the same bounded allocation.
    for _ in 0..2 {
        let mut read=0;let mut write=0;
        while read<text.len() {
            let value=if text[read]==b'%' && read+2<text.len()
                && let (Some(a),Some(b))=((text[read+1] as char).to_digit(16),(text[read+2] as char).to_digit(16)) {
                read+=3;(a*16+b) as u8
            } else {let value=if text[read]==b'+' {b' '} else {text[read]};read+=1;value};
            text[write]=value;write+=1;
        }
        text.truncate(write);
    }
    text.make_ascii_lowercase();
    std::borrow::Cow::Owned(text)
}
pub fn extract ( sample: &[u8], total: usize ) -> [f32;CONTENT_COUNT] {
    let text=decoded(sample);
    let mut features=[0.0;CONTENT_COUNT];
    features[0]=sample.len() as f32;
    features[5]=memchr::memchr_iter(b'%',sample).count() as f32;
    for value in text.iter() {
        features[1]+=f32::from(!value.is_ascii());
        features[2]+=f32::from(value.is_ascii_control());
        features[3]+=f32::from(value.is_ascii_digit());
        features[4]+=f32::from(value.is_ascii_punctuation());
        features[6]+=f32::from(matches!(*value,39|34|96));
        features[7]+=f32::from(b"<>{}[]();|&".contains(value));
    }
    for value in &mut features[1..5] {*value/=text.len().max(1) as f32;}
    for (index,patterns) in PATTERNS.iter().enumerate() {
        for pattern in patterns {
            let mut offset=0;
            while let Some(found)=pattern.find(&text[offset..]) {features[index+8]+=1.0;offset+=found+1;}
        }
    }
    features[14]=f32::from(total>sample.len());
    features[15]=f32::from(!sample.is_empty());
    let mut counts=[[0u32;LEXICAL_BUCKETS];2];
    for index in 1..text.len() {
        let pair=((2166136261u32^u32::from(text[index-1])).wrapping_mul(16777619)^u32::from(text[index])).wrapping_mul(16777619);
        counts[0][(pair^(pair>>16)) as usize & (LEXICAL_BUCKETS-1)]+=1;
        if let Some(next)=text.get(index+1) {
            let triple=(pair^u32::from(*next)).wrapping_mul(16777619);
            counts[1][(triple^(triple>>16)) as usize & (LEXICAL_BUCKETS-1)]+=1;
        }
    }
    for (group,counts) in counts.into_iter().enumerate() {
        let length=text.len().saturating_sub(group+1).max(1) as f32;
        for (index,count) in counts.into_iter().enumerate() {features[16+group*LEXICAL_BUCKETS+index]=count as f32/length;}
    }
    features
}

#[cfg(test)]
mod tests {
    use super::*;
    #[derive(serde::Deserialize)]
    struct Case { sample: Vec<u8>, total: usize, expected: Vec<f32> }
    #[test]
    fn extraction_matches_python_including_double_encoding_and_binary_input () {
        let cases:Vec<Case>=serde_json::from_str(include_str!("../../../../model/weights/content-parity.json")).unwrap();
        for case in cases {
            assert_eq!(case.expected.len(),CONTENT_COUNT);
            for (actual,expected) in extract(&case.sample,case.total).iter().zip(case.expected) {assert!((actual-expected).abs()<1e-6);}
        }
    }
}
