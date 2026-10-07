use http::Method;
use serde::Deserialize;
use serde_json::json;

use crate::app::{Ban, BackendEvent, Block, Decisions, Identity, Overlay};
use crate::core::time::Clock;
use crate::http::variable::Keyval;
use crate::config::{BackendConfig, Config};
use crate::core::parse::Json;
use crate::core::rt::Rt;
use crate::http::body::Body;
use crate::http::response::{Res, Response};
use super::arch::Admin;

const CONTRACT: &str = include_str!("../../../contracts/v1.json");

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Purge {
    kind : String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Revoke {
    key : String,
}

#[derive(Deserialize, Default)]
#[serde(default, deny_unknown_fields)]
struct Evict {
    key    : Option<String>,
    all    : bool,
    host   : Option<String>,
    prefix : Option<String>,
    tag    : Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Member {
    pool    : String,
    backend : BackendConfig,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Departure {
    pool    : String,
    address : String,
}

impl Admin {

    fn adjust ( &self, change: impl FnOnce(&Config, &mut Overlay) -> Result<(), &'static str> ) -> Res<Body> {

        match self.state.adjust(change) {
            Err(reason) => Self::json(409, &json!({ "error": reason })),
            Ok(Ok(version)) => Self::json(200, &json!({ "ok": true, "version": version, "edits": self.state.edits() })),
            Ok(Err(error)) => Self::json(422, &json!({ "error": "rejected", "message": error.to_string() })),
        }

    }

    pub(super) async fn api ( &self, method: &Method, path: &str, body: &[u8] ) -> Res<Body> {

        let decisions = self.state.decisions();

        match ( method.as_str(), path ) {
            ( "GET", "/state" ) => Self::json(200, &self.state()),
            ( "GET", "/metrics" ) => self.metrics(),
            ( "GET", "/contracts" ) => {

                let mut response = Response::bytes(200, "application/json", CONTRACT);

                Self::harden(response.headers_mut());

                response

            }
            ( "GET", "/decisions" ) => match &decisions {
                Some(decisions) => match decisions.list(200) {
                    Ok(items) => Self::json(200, &json!({ "items": items })),
                    Err(_) => Self::json(503, &json!({ "error": "decision_store_unavailable" })),
                },
                None => Self::json(200, &json!({ "items": [] })),
            },
            ( "POST", "/blocks" ) => {

                let Some(decisions) = &decisions else { return Self::json(409, &json!({ "error": "storage_disabled" })); };
                let Ok(block) = Json::parse::<Block>(body) else { return Self::json(400, &json!({ "error": "invalid_block" })); };
                let runtime = self.state.load();

                if block.config_version != self.config_version(&runtime.snapshot) { return Self::json(409, &json!({ "error": "configuration_changed" })); }

                let Some(route) = runtime.snapshot.routes.iter().find(|route| route.spec.name == block.route) else { return Self::json(404, &json!({ "error": "unknown_route" })); };

                if !route.policy.decisions { return Self::json(409, &json!({ "error": "decision_cache_disabled" })); }

                let Some(actor) = Decisions::parse_key(&block.actor) else { return Self::json(400, &json!({ "error": "invalid_actor" })); };

                let reason_ok = !block.reason.is_empty() && block.reason.len() <= 128 && !block.reason.chars().any(char::is_control);
                let id_ok = block.request_id.as_deref().is_none_or(|id| Identity::looks_like_uuid(id.as_bytes()));

                if !(1..=decisions.deny_ttl_ms()).contains(&block.ttl_ms) || !reason_ok || !id_ok { return Self::json(400, &json!({ "error": "invalid_block_scope" })); }

                let ( key, verdict ) = decisions.verdict(&route.spec.name, &actor, block.ttl_ms, &block.reason, "operator", block.request_id.clone());
                let hex = verdict.key.clone();

                match decisions.block(key, verdict).await {
                    Ok(Ok(())) => Self::json(200, &json!({ "ok": true, "key": hex, "config_version": block.config_version })),
                    _ => Self::json(503, &json!({ "error": "decision_store_unavailable", "outcome": "unknown" })),
                }

            }
            ( "POST", "/blocks/revoke" ) => {

                let Some(decisions) = &decisions else { return Self::json(409, &json!({ "error": "storage_disabled" })); };
                let Some(key) = Json::parse::<Revoke>(body).ok().and_then(|revoke| Decisions::parse_key(&revoke.key)) else { return Self::json(400, &json!({ "error": "invalid_key" })); };

                match decisions.revoke(key).await {
                    Ok(Ok(())) => Self::json(200, &json!({ "ok": true, "revoked": true })),
                    _ => Self::json(503, &json!({ "error": "decision_store_unavailable", "outcome": "unknown" })),
                }

            }
            ( "GET", "/cancellations" | "/backend/cancellations" ) => Self::json(200, &json!({ "items": [] })),
            ( "GET", rest ) if rest.starts_with("/requests/") => {

                let id = &rest["/requests/".len()..];

                if !Identity::looks_like_uuid(id.as_bytes()) { return Self::json(400, &json!({ "error": "invalid_request_id" })); }

                let events = self.telemetry.journey(id).map(|journey| journey.events).unwrap_or_default();

                Self::json(200, &json!({ "events": events }))

            }
            ( "POST", "/backend/events" ) => {

                let Ok(event) = Json::parse::<BackendEvent>(body) else { return Self::json(400, &json!({ "error": "invalid_backend_event" })); };

                if !event.valid() { return Self::json(400, &json!({ "error": "invalid_backend_event" })); }

                match self.telemetry.backend(event) {
                    Some(()) => Self::json(202, &json!({ "accepted": true })),
                    None => Self::json(409, &json!({ "error": "request_not_active_or_event_budget_exhausted" })),
                }

            }
            ( "POST", "/reload" ) => { Rt::reload(); Self::json(202, &json!({ "accepted": true })) }
            ( "GET", "/upstreams" ) => Self::json(200, &json!({ "items": self.state()["upstreams"], "edits": self.state.edits() })),
            ( "POST" | "PUT", "/upstreams" ) => match serde_json::from_slice::<Member>(body) {
                Ok(member) => self.adjust(|config, overlay| {

                    if !config.pools.contains_key(&member.pool) { return Err("unknown_pool"); }

                    overlay.join(&member.pool, member.backend);

                    Ok(())

                }),
                Err(error) => Self::json(400, &json!({ "error": "invalid_backend", "message": error.to_string() })),
            },
            ( "DELETE", "/upstreams" ) => match serde_json::from_slice::<Departure>(body) {
                Ok(departure) => self.adjust(|config, overlay| {

                    let pool = config.pools.get(&departure.pool).ok_or("unknown_pool")?;

                    match ( pool.backends.iter().any(|backend| backend.address.to_string() == departure.address), pool.backends.len() ) {
                        ( false, _ ) => Err("unknown_backend"),
                        ( true, 1 ) => Err("last_backend"),
                        ( true, _ ) => { overlay.leave(&departure.pool, &departure.address); Ok(()) }
                    }

                }),
                Err(error) => Self::json(400, &json!({ "error": "invalid_backend", "message": error.to_string() })),
            },
            ( "POST", "/upstreams/reset" ) => self.adjust(|_, overlay| { overlay.clear(); Ok(()) }),
            ( "GET", "/cache" ) => match self.state.cache() {
                Some(store) => Self::json(200, &store.describe()),
                None => Self::json(200, &json!({ "enabled": false })),
            },
            ( "DELETE", "/cache" ) => match ( self.state.cache(), serde_json::from_slice::<Evict>(body) ) {
                ( None, _ ) => Self::json(409, &json!({ "error": "cache_disabled" })),
                ( Some(store), Ok(Evict { key: Some(key), .. }) ) => Self::json(200, &json!({ "purged": store.purge(&bytes::Bytes::from(key)) })),
                ( Some(store), Ok(Evict { all: true, .. }) ) => { store.clear(); Self::json(200, &json!({ "purged": true })) }
                ( Some(store), Ok(Evict { host, prefix, tag, .. }) ) if prefix.as_ref().is_some_and(|prefix| prefix.starts_with('/')) || tag.as_ref().is_some_and(|tag| !tag.is_empty() && !tag.contains([',', ' '])) => Self::json(200, &json!({ "purged": true, "bans": store.ban(Ban { host: host.map(Into::into), prefix: prefix.map(Into::into), tag: tag.map(Into::into), at_ms: Clock::wall_ms(std::time::Instant::now()) }) })),
                _ => Self::json(400, &json!({ "error": "key_or_all_required" })),
            },
            ( "POST", "/cache/purge" ) => {

                let Ok(purge) = Json::parse::<Purge>(body) else { return Self::json(400, &json!({ "error": "invalid_purge" })); };

                if !matches!(purge.kind.as_str(), "all" | "decisions" | "scores" | "responses") { return Self::json(400, &json!({ "error": "unknown_cache" })); }

                if matches!(purge.kind.as_str(), "all" | "decisions") && let Some(decisions) = &decisions && decisions.purge().is_err() {

                    return Self::json(503, &json!({ "error": "decision_store_unavailable" }));

                }

                Self::json(200, &json!({ "ok": true, "persistent_decisions_preserved": true }))

            }
            ( "POST", "/cancellations" | "/backend/cancellations/ack" ) => Self::json(409, &json!({ "error": "storage_disabled" })),
            ( verb, path ) if path.starts_with("/keyval/") => {

                let Some(zone) = Keyval::zone(&path["/keyval/".len()..], false) else { return Self::json(404, &json!({ "error": "unknown_keyval" })); };

                match ( verb, serde_json::from_slice::<std::collections::BTreeMap<String, String>>(body) ) {
                    ( "GET", _ ) => Self::json(200, &json!({ "items": Keyval::list(&zone).into_iter().collect::<std::collections::BTreeMap<_, _>>() })),
                    ( "PUT" | "POST", Ok(pairs) ) => { for ( key, value ) in &pairs { Keyval::set(&zone, key.as_bytes(), value.as_bytes()); } Self::json(200, &json!({ "ok": true, "stored": pairs.len() })) }
                    ( "DELETE", Ok(pairs) ) => { for key in pairs.keys() { Keyval::remove(&zone, Some(key.as_bytes())); } Self::json(200, &json!({ "ok": true })) }
                    ( "DELETE", Err(_) ) if body.is_empty() => { Keyval::remove(&zone, None); Self::json(200, &json!({ "ok": true })) }
                    _ => Self::json(400, &json!({ "error": "invalid_keyval" })),
                }

            }
            _ => Self::json(404, &json!({ "error": "not_found" })),
        }

    }

}
