use std::sync::atomic::Ordering;

use super::arch::Tally;

impl Tally {

    pub fn add ( &self, amount: u64 ) {

        self.value.store(self.value.load(Ordering::Relaxed).wrapping_add(amount), Ordering::Relaxed);

    }

    pub fn sub ( &self, amount: u64 ) {

        self.value.store(self.value.load(Ordering::Relaxed).wrapping_sub(amount), Ordering::Relaxed);

    }

    pub fn get ( &self ) -> u64 {

        self.value.load(Ordering::Relaxed)

    }

}
