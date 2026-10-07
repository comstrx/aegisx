use actix_web::HttpResponse;
use serde::Deserialize;
use serde_json::json;
use crate::core::domain::{Block, hex};
use crate::module::{decision::Engine, verdict::Verdict};
use super::{server::State, api::json_response};

fn key ( value: &str ) -> Option<[u8;32]> {
    if value.len()!=64 || !value.is_ascii() { return None; }
    (0..64).step_by(2).map(|index|u8::from_str_radix(&value[index..index+2],16).ok())
        .collect::<Option<Vec<_>>>()?.try_into().ok()
}
pub(super) async fn execute ( method: &str, path: &str, body: &[u8], state: &State ) -> HttpResponse {
    let repository = state.services.engine.verdicts.as_ref();
    match (method,path) {
        ("GET","/cancellations" | "/backend/cancellations") => {
            let Some(repository)=repository else { return json_response(200,json!({"items":[]})); };
            match repository.cancellations().await {
                Ok(items) => json_response(200,json!({"items":items.into_iter().filter(|item| path!="/backend/cancellations" || matches!(item.state.as_str(),"requested"|"accepted")).collect::<Vec<_>>()})),
                Err(_) => json_response(503,json!({"error":"cancellations_unavailable"})),
            }
        }
        ("POST","/backend/cancellations/ack") => {
            #[derive(Deserialize)] #[serde(deny_unknown_fields)] struct Ack { action_id:String, state:String }
            let Some(repository)=repository else { return json_response(409,json!({"error":"storage_disabled"})); };
            let Ok(ack)=serde_json::from_slice::<Ack>(body) else { return json_response(400,json!({"error":"invalid_acknowledgement"})); };
            match repository.acknowledge(ack.action_id,ack.state).await {
                Ok(action) => json_response(200,json!({"action":action})),
                Err(_) => json_response(409,json!({"error":"unknown_expired_or_final_action"})),
            }
        }
        ("POST","/cancellations") => {
            #[derive(Deserialize)] #[serde(deny_unknown_fields)] struct Cancel { request_id:String, route:String }
            let Some(repository)=repository else { return json_response(409,json!({"error":"storage_disabled"})); };
            let Ok(action)=serde_json::from_slice::<Cancel>(body) else { return json_response(400,json!({"error":"invalid_cancellation"})); };
            if uuid::Uuid::parse_str(&action.request_id).is_err() { return json_response(400,json!({"error":"invalid_request_id"})); }
            let snapshot=state.services.current.load_full();
            let Some(route)=snapshot.routes.iter().find(|route|route.spec.name==action.route && route.spec.cancellation)
                else { return json_response(409,json!({"error":"route_does_not_support_cancellation"})); };
            let action=crate::module::verdict::Cancellation::new(action.request_id,action.route,"operator_request".into(),route.spec.cancellation_ttl_ms);
            match repository.cancel(action).await {
                Ok(action) => { state.services.cancellation_notice(&action); json_response(202,json!({"action":action})) }
                Err(_) => json_response(503,json!({"error":"cancellation_persistence_unavailable","outcome":"unknown"})),
            }
        }
        ("GET","/decisions") => match repository {
            Some(repository) => match repository.list().await {
                Ok(items) => json_response(200,json!({"items":items})),
                Err(_) => json_response(503,json!({"error":"decision_store_unavailable"})),
            },
            None => json_response(200,json!({"items":[]})),
        },
        ("POST","/cache/purge") => {
            #[derive(Deserialize)] #[serde(deny_unknown_fields)] struct Purge { kind:String }
            let Ok(action)=serde_json::from_slice::<Purge>(body) else { return json_response(400,json!({"error":"invalid_purge"})); };
            let caches=&state.services.caches;
            match action.kind.as_str() {
                "all" => { caches.scores.invalidate_all(); caches.purge_responses(); }
                "decisions" => {},
                "scores" => caches.scores.invalidate_all(),
                "responses" => caches.purge_responses(),
                _ => return json_response(400,json!({"error":"unknown_cache"})),
            }
            if matches!(action.kind.as_str(),"all"|"decisions") && let Some(repository)=repository
                && repository.purge().await.is_err() { return json_response(503,json!({"error":"decision_store_unavailable"})); }
            json_response(200,json!({"ok":true,"persistent_decisions_preserved":true}))
        }
        ("POST","/blocks/revoke") => {
            #[derive(Deserialize)] #[serde(deny_unknown_fields)] struct Revoke { key:String }
            let Some(repository)=repository else { return json_response(409,json!({"error":"storage_disabled"})); };
            let Some(key)=serde_json::from_slice::<Revoke>(body).ok().and_then(|action|key(&action.key))
                else { return json_response(400,json!({"error":"invalid_key"})); };
            match repository.revoke(key).await {
                Ok(()) => json_response(200,json!({"ok":true,"revoked":true})),
                Err(_) => json_response(503,json!({"error":"decision_store_unavailable","outcome":"unknown"})),
            }
        }
        ("POST","/blocks") => {
            let Some(repository)=repository else { return json_response(409,json!({"error":"storage_disabled"})); };
            let Ok(action)=serde_json::from_slice::<Block>(body) else { return json_response(400,json!({"error":"invalid_block"})); };
            let snapshot=state.services.current.load_full();
            if action.config_version!=snapshot.version { return json_response(409,json!({"error":"configuration_changed"})); }
            let Some(route)=snapshot.routes.iter().chain(snapshot.fallback.iter()).find(|route|route.spec.name==action.route)
                else { return json_response(404,json!({"error":"unknown_route"})); };
            if !Engine::cache_enabled(&snapshot,route) { return json_response(409,json!({"error":"decision_cache_disabled"})); }
            let Some(actor)=key(&action.actor) else { return json_response(400,json!({"error":"invalid_actor"})); };
            if !(1..=snapshot.config.cache.deny_ttl_ms).contains(&action.ttl_ms) || action.reason.is_empty()
                || action.reason.len()>128 || action.reason.chars().any(char::is_control)
                || action.request_id.as_ref().is_some_and(|id|uuid::Uuid::parse_str(id).is_err())
            { return json_response(400,json!({"error":"invalid_block_scope"})); }
            let key=Engine::key(&snapshot,route,&actor);
            let now=crate::core::time::now_ms();
            let verdict=Verdict { key:hex(&key),actor:action.actor,route:action.route,reason:action.reason,
                source:"operator".into(),created_ms:now,expires_ms:now+action.ttl_ms,request_id:action.request_id };
            match repository.put(key,verdict).await {
                Ok(_) => json_response(200,json!({"ok":true,"key":hex(&key),"config_version":snapshot.version})),
                Err(_) => json_response(503,json!({"error":"decision_store_unavailable","outcome":"unknown"})),
            }
        }
        _ => json_response(404,json!({"error":"not_found"})),
    }
}
