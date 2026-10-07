use std::time::Instant;

use crate::app::Identity;
use crate::config::ServerConfig;
use crate::core::time::Clock;
use crate::http::server::Session;
use super::arch::{Bridged, Context, Handler, Ticket};

impl Drop for Ticket {

    fn drop ( &mut self ) {

        self.context.stats.inflight.sub(1);
        self.context.inflight.set(self.context.inflight.get().saturating_sub(1));
        self.context.served.set(true);
        self.context.seen.set(Clock::recent().max(self.context.seen.get()));

        if self.counted { self.context.stats.settle(); }

        if let Some(actor) = &self.actor { actor.release(); }

        if let Some(pending) = self.pending.take() { pending.submit(); }

        drop(self.fill.take());

        if let Some(( log, entry )) = self.access.take() { log.record(&entry); }

    }

}

impl Handler {

    pub fn tunnels ( &self ) -> usize {

        self.tunnels.get()

    }

    pub(super) fn bridged ( &self ) -> Bridged {

        self.tunnels.set(self.tunnels.get() + 1);

        Bridged { count: self.tunnels.clone() }

    }

}

impl Drop for Bridged {

    fn drop ( &mut self ) {

        self.count.set(self.count.get().saturating_sub(1));

    }

}

impl Context {

    pub(super) fn spent ( &self, server: &ServerConfig ) -> bool {

        let count = self.count.get() + 1;

        self.count.set(count);

        (server.keepalive_requests > 0 && count >= server.keepalive_requests) || (server.keepalive_time_ms > 0 && Clock::recent().saturating_duration_since(self.born).as_millis() as u64 >= server.keepalive_time_ms)

    }

}

impl Session for Context {

    fn idle ( &self ) -> Option<( Instant, bool )> {

        (self.inflight.get() == 0).then(|| ( self.seen.get(), self.served.get() ))

    }

    fn identify ( &self, certificate: &[u8] ) {

        *self.client.borrow_mut() = Identity::certificate(certificate).map(Box::new);

    }

}
