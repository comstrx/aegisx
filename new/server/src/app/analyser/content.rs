use std::borrow::Cow;

use crate::app::{CONTENT_FIXED, Schema};

pub struct Content;

impl Content {

    pub fn extract ( sample: &[u8], total: usize, schema: &Schema ) -> Vec<f32> {

        let text = Self::decoded(sample);
        let buckets = schema.buckets;
        let mut features = vec![0.0f32; CONTENT_FIXED + 2 * buckets];

        features[0] = sample.len() as f32;
        features[5] = memchr::memchr_iter(b'%', sample).count() as f32;

        for value in text.iter() {

            features[1] += f32::from(!value.is_ascii());
            features[2] += f32::from(value.is_ascii_control());
            features[3] += f32::from(value.is_ascii_digit());
            features[4] += f32::from(value.is_ascii_punctuation());
            features[6] += f32::from(matches!(*value, b'\'' | b'"' | b'`'));
            features[7] += f32::from(b"<>{}[]();|&".contains(value));

        }

        for value in &mut features[1..5] { *value /= text.len().max(1) as f32; }

        for ( index, group ) in schema.patterns.iter().enumerate() {

            for pattern in group {

                let mut offset = 0;

                while let Some(found) = pattern.find(&text[offset..]) {

                    features[index + 8] += 1.0;
                    offset += found + 1;

                }

            }

        }

        features[14] = f32::from(total > sample.len());
        features[15] = f32::from(!sample.is_empty());

        let mut counts = vec![0u32; 2 * buckets];
        let mask = buckets - 1;

        for index in 1..text.len() {

            let pair = ((2_166_136_261u32 ^ u32::from(text[index - 1])).wrapping_mul(16_777_619) ^ u32::from(text[index])).wrapping_mul(16_777_619);

            counts[(pair ^ (pair >> 16)) as usize & mask] += 1;

            if let Some(next) = text.get(index + 1) {

                let triple = (pair ^ u32::from(*next)).wrapping_mul(16_777_619);

                counts[buckets + ((triple ^ (triple >> 16)) as usize & mask)] += 1;

            }

        }

        for group in 0..2 {

            let length = text.len().saturating_sub(group + 1).max(1) as f32;

            for index in 0..buckets { features[CONTENT_FIXED + group * buckets + index] = counts[group * buckets + index] as f32 / length; }

        }

        features

    }

    fn decoded ( input: &[u8] ) -> Cow<'_, [u8]> {

        if memchr::memchr2(b'%', b'+', input).is_none() && !input.iter().any(u8::is_ascii_uppercase) { return Cow::Borrowed(input); }

        let mut text = input.to_vec();

        for _ in 0..2 {

            let mut read = 0;
            let mut write = 0;

            while read < text.len() {

                let value = match text[read] {
                    b'%' if read + 2 < text.len() => match ( (text[read + 1] as char).to_digit(16), (text[read + 2] as char).to_digit(16) ) {
                        ( Some(high), Some(low) ) => { read += 3; (high * 16 + low) as u8 }
                        _ => { read += 1; b'%' }
                    },
                    b'+' => { read += 1; b' ' }
                    other => { read += 1; other }
                };

                text[write] = value;
                write += 1;

            }

            text.truncate(write);

        }

        text.make_ascii_lowercase();

        Cow::Owned(text)

    }

}
