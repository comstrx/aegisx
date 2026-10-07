use std::fmt;

use crate::core::env::Env;
use crate::core::error::{AppError, AppResult};
use super::arch::Secret;

impl Secret {

    pub fn from_env ( name: &str, min: usize, max: usize ) -> AppResult<Self> {

        let value = Env::get(name).ok_or_else(|| AppError::config(name, "environment variable is missing or empty"))?;

        if !(min..=max).contains(&value.len()) || !value.bytes().all(|byte| byte.is_ascii_graphic()) {

            return Err(AppError::config(name, format!("secret must be {min}-{max} printable ascii bytes")));

        }

        Ok(Self { bytes: value.into_bytes().into_boxed_slice() })

    }

    pub fn matches ( &self, candidate: &[u8] ) -> bool {

        if candidate.len() != self.bytes.len() { return false; }

        self.bytes.iter().zip(candidate).fold(0u8, |acc, ( left, right )| acc | (left ^ right)) == 0

    }

    pub fn same ( &self, other: &Self ) -> bool {

        self.matches(&other.bytes)

    }

}

impl fmt::Debug for Secret {

    fn fmt ( &self, formatter: &mut fmt::Formatter<'_> ) -> fmt::Result {

        formatter.write_str("Secret(***)")

    }

}
