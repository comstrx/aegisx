use std::sync::atomic::Ordering;
use std::time::Instant;
use pingora::proxy::Session;
use serde_json::json;
use super::{Context, Proxy};

impl Proxy {

    pub(super) async fn cached_response ( &self, session: &mut Session, context: &mut Context ) -> pingora::Result<bool> {

        if context.snapshot.config.identity.actor_header.as_ref().is_some_and(|name| session.req_header().headers.contains_key(name)) { return Ok(false); }
        let Some(route) = &context.route else { return Ok(false); };
        if !context.snapshot.config.cache.responses || !route.spec.response_cache.unwrap_or(true) { return Ok(false); }
        context.cache_key = self.caches.response_key(&context.snapshot.version, &route.spec.name, session.req_header());
        let Some(key) = context.cache_key else { return Ok(false); };
        let Some(cached) = self.caches.responses.get(&key) else { return Ok(false); };
        if cached.expires <= Instant::now() { self.caches.responses.invalidate(&key); return Ok(false); }
        let mut header = cached.header.clone();
        Self::response_identity(&mut header, context)?;
        header.insert_header("age", (cached.initial_age + cached.created.elapsed().as_secs()).to_string())?;
        self.caches.response_hits.fetch_add(1, Ordering::Relaxed);
        context.cache_hit = true;
        self.record(context, "response_cache_hit", |_| json!({ "status": 200 }));
        session.write_response_header(Box::new(header), false).await?;
        session.write_response_body(Some(cached.body.clone()), true).await?;

        Ok(true)

    }

}
