use crate::core::domain::Actor;
use super::{History, HistoryGuard};

impl History {

    pub fn now ( &self ) -> u64 { self.started.elapsed().as_millis() as u64 }

}

impl HistoryGuard {

    pub fn finish ( &mut self, failed: bool, blocked: bool ) {

        let elapsed = self.started.elapsed().as_millis() as u64;
        let now = self.history.now();
        if self.global { self.history.global.finish(self.actor, now, elapsed, failed, blocked); self.global = false; }
        if let Some(route) = self.route.take() { self.history.routes.finish((route, self.actor), now, elapsed, failed, blocked); }

    }

    pub(super) fn new ( history: std::sync::Arc<History>, actor: Actor ) -> Self {
        Self { history, actor, global: false, route: None, started: std::time::Instant::now() }
    }

}

impl Drop for HistoryGuard {
    fn drop ( &mut self ) { self.finish(true, false); }
}
