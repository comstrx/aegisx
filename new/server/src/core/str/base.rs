use std::collections::HashSet;
use std::sync::Mutex;

use super::arch::{INTERNED, Str};

impl Str {

    pub fn intern ( text: &str ) -> &'static str {

        let mut interned = INTERNED.get_or_init(|| Mutex::new(HashSet::new())).lock().unwrap_or_else(|poisoned| poisoned.into_inner());

        if let Some(known) = interned.get(text) { return known; }

        let leaked: &'static str = Box::leak(text.to_owned().into_boxed_str());

        interned.insert(leaked);

        leaked

    }

}
