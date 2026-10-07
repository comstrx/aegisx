use std::sync::Arc;
use tokio::sync::{Mutex,MutexGuard};

/// One FIFO gate for database writers; WAL readers never acquire it.
#[derive(Clone,Default)]
pub struct WriteGate ( Arc<Mutex<()>> );
impl WriteGate {
    pub fn lock ( &self ) -> MutexGuard<'_,()> {self.0.blocking_lock()}
}
