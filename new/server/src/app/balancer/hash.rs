use std::hash::{BuildHasher, Hasher};
use std::sync::Arc;

use foldhash::fast::FixedState;
use http::header::{HeaderMap, HeaderValue, SET_COOKIE};

use crate::config::StickyConfig;
use super::arch::{Backend, HashKey, Sticky};

const SEED: u64 = 0x5eed_a3a3_9f1d_77c1;

impl HashKey {

    pub fn rendezvous ( backends: &[Arc<Backend>], hash: u64, open: &dyn Fn(&Arc<Backend>) -> bool ) -> Option<usize> {

        let mut best: Option<( usize, f64 )> = None;

        for ( index, backend ) in backends.iter().enumerate() {

            if !open(backend) { continue; }

            let mut mixer = FixedState::with_seed(SEED ^ (index as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15)).build_hasher();

            mixer.write_u64(hash);

            let unit = (mixer.finish() >> 11) as f64 / (1u64 << 53) as f64;
            let score = f64::from(backend.weight) / -(unit.max(f64::MIN_POSITIVE)).ln();

            if best.is_none_or(|( _, current )| score > current) { best = Some(( index, score )); }

        }

        best.map(|( index, _ )| index)

    }

}

impl Sticky {

    pub fn compile ( spec: &StickyConfig ) -> Self {

        let mut suffix = format!("; Path={}", spec.path);

        if spec.ttl_ms > 0 { suffix.push_str(&format!("; Max-Age={}", spec.ttl_ms.div_ceil(1_000))); }

        if spec.http_only { suffix.push_str("; HttpOnly"); }

        if spec.secure { suffix.push_str("; Secure"); }

        if let Some(mode) = &spec.same_site { suffix.push_str(&format!("; SameSite={mode}")); }

        Self { cookie: spec.cookie.as_str().into(), suffix: suffix.into() }

    }

    pub fn wanted ( &self, headers: &HeaderMap, backends: &[Arc<Backend>] ) -> Option<usize> {

        let value = HashKey::cookie(headers, &self.cookie)?;

        backends.iter().position(|backend| backend.id.as_bytes() == value)

    }

    pub fn issue ( &self, headers: &mut HeaderMap, backend: &Backend ) {

        if let Ok(value) = HeaderValue::from_str(&format!("{}={}{}", self.cookie, backend.id, self.suffix)) { headers.append(SET_COOKIE, value); }

    }

}

impl Backend {

    pub fn identity ( pool: &str, address: &str ) -> Box<str> {

        let mut hasher = FixedState::with_seed(SEED).build_hasher();

        hasher.write(pool.as_bytes());
        hasher.write(address.as_bytes());

        format!("{:016x}", hasher.finish()).into_boxed_str()

    }

}
