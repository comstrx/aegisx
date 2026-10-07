use std::collections::HashMap;
use std::sync::Mutex;

use super::arch::{SLOTS, Slot};

impl Slot {

    pub fn of ( name: &str ) -> usize {

        let mut slots = SLOTS.get_or_init(|| Mutex::new(HashMap::new())).lock().unwrap_or_else(|poisoned| poisoned.into_inner());

        if let Some(slot) = slots.get(name) { return *slot; }

        let slot = slots.len();

        slots.insert(name.into(), slot);

        slot

    }

}
