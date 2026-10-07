use actix_web::{HttpRequest, HttpResponse, web};
use serde_json::json;

use super::server::State;

include!(concat!(env!("OUT_DIR"), "/panel_assets.rs"));

pub(super) fn json_response ( code: u16, value: serde_json::Value ) -> HttpResponse {
    HttpResponse::build(actix_web::http::StatusCode::from_u16(code).unwrap())
        .insert_header(("cache-control", "no-store")).insert_header(("x-content-type-options", "nosniff")).json(value)
}

pub(super) async fn dispatch ( request: HttpRequest, body: web::Bytes, state: web::Data<State> ) -> HttpResponse {

    let address = state.config.listen;
    let authority = request.headers().get("host").and_then(|value| value.to_str().ok()).unwrap_or("");
    if authority != address.to_string() && authority != format!("localhost:{}", address.port()) {
        return json_response(400, json!({"error":"invalid_control_host"}));
    }
    if let Some(origin) = request.headers().get("origin")
        && origin.as_bytes() != format!("http://{authority}").as_bytes()
    { return json_response(403, json!({"error":"cross_origin_denied"})); }
    let path = request.path();
    let api = state.config.api_prefix.as_str();
    if let Some(path) = path.strip_prefix(api).filter(|path| path.starts_with('/')) {
        let token = request.headers().get("authorization").and_then(|value| value.to_str().ok())
            .and_then(|value| value.strip_prefix("Bearer ")).unwrap_or("");
        let expected = if path.starts_with("/backend/") { state.backend_token.as_deref().unwrap_or("") } else { &state.token };
        if expected.is_empty() || token.len() != expected.len() || !openssl::memcmp::eq(token.as_bytes(), expected.as_bytes()) {
            return json_response(401, json!({"error":"unauthorized"}));
        }
        return api_request(request.method().as_str(), path, &body, &state).await;
    }
    if request.method().as_str() != "GET" || !state.config.panel { return json_response(404, json!({"error":"not_found"})); }
    if path == "/aegisx-bootstrap.json" { return json_response(200, json!({"api_prefix":api,"schema_version":1})); }
    let path = if path == "/" { "/index.html" } else { path };
    let Some((_, bytes, mime)) = ASSETS.iter().find(|(name, _, _)| *name == path) else {
        return json_response(404, json!({"error":"not_found"}));
    };
    HttpResponse::Ok().content_type(*mime)
        .insert_header(("x-content-type-options","nosniff"))
        .insert_header(("referrer-policy","no-referrer"))
        .insert_header(("content-security-policy","default-src 'self'; script-src 'self' 'unsafe-inline'; style-src 'self' 'unsafe-inline'; connect-src 'self'; object-src 'none'; base-uri 'none'; frame-ancestors 'none'"))
        .insert_header(("cache-control",if path.starts_with("/_next/static/") { "public, max-age=31536000, immutable" } else { "no-store" }))
        .body(*bytes)

}

async fn api_request ( method: &str, path: &str, body: &[u8], state: &State ) -> HttpResponse {
    match (method,path) {
        ("GET","/state") => json_response(200,state.services.state()),
        ("GET","/contracts") => HttpResponse::Ok().content_type("application/json")
            .insert_header(("cache-control","no-store")).body(include_str!("../../../contracts/v1.json")),
        ("POST","/backend/events") => {
            let Ok(event)=serde_json::from_slice::<crate::module::lifecycle::BackendEvent>(body)
                else { return json_response(400,json!({"error":"invalid_backend_event"})); };
            if !event.valid() { return json_response(400,json!({"error":"invalid_backend_event"})); }
            let Some(capture) = state.services.journeys.backend(event.clone()) else {
                return json_response(409,json!({"error":"request_not_active_or_event_budget_exhausted"}));
            };
            if capture && state.services.current.load().config.telemetry.enabled { state.services.telemetry.publish(crate::module::storage::Event { request_id:event.request_id.clone(),sequence:0,
                stage:"backend_reported".into(),timestamp_ms:crate::core::time::now_ms(),elapsed_ms:0,details:json!(event) }); }
            json_response(202,json!({"accepted":true}))
        }
        ("GET",path) if path.starts_with("/requests/") => {
            let id=path.trim_start_matches("/requests/").to_owned();
            if uuid::Uuid::parse_str(&id).is_err() { return json_response(400,json!({"error":"invalid_request_id"})); }
            let path=state.services.current.load().config.store.clone();
            let Some(path)=path else { return json_response(409,json!({"error":"storage_disabled"})); };
            match tokio::task::spawn_blocking(move || crate::module::storage::Store::inspect(&path,Some(&id),200)).await {
                Ok(Ok(events)) => json_response(200,json!({"events":events})),
                _ => json_response(503,json!({"error":"history_unavailable"})),
            }
        }
        _ => super::actions::execute(method,path,body,state).await,
    }
}
