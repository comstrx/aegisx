mod arch;
mod guard;
mod routes;
mod store;

use std::time::Instant;

use actix_web::dev::Service;
use actix_web::http::header::{HeaderName, HeaderValue};
use actix_web::{App, HttpServer, web};
use mimalloc::MiMalloc;

use crate::arch::{Settings, State};
use crate::routes::Routes;

#[global_allocator]
static GLOBAL: MiMalloc = MiMalloc;

const REQUEST_ID: HeaderName = HeaderName::from_static("x-request-id");
const SERVER_TIMING: HeaderName = HeaderName::from_static("server-timing");

#[actix_web::main]
async fn main () -> std::io::Result<()> {

    let settings = Settings::load();
    let state = State::open(&settings).await.map_err(std::io::Error::other)?;

    if settings.tokens > 0 {

        let expires = State::now() + 31_536_000;
        let members = state.members(settings.tokens as i64).await.map_err(std::io::Error::other)?;
        let tenants: std::collections::HashMap<i32, &str> = state.tenants.iter().map(|( slug, id )| ( *id, slug.as_str() )).collect();

        println!("return {{");

        for ( user, tenant ) in members { println!("  {{ tenant = \"{}\", token = \"{}\" }},", tenants.get(&tenant).copied().unwrap_or("t1"), state.issue(user, tenant, expires)); }

        println!("}}");

        return Ok(());

    }

    let shared = web::Data::new(state);

    HttpServer::new(move || {

        App::new()
            .app_data(shared.clone())
            .app_data(web::JsonConfig::default().limit(65_536))
            .app_data(web::PayloadConfig::default().limit(64 * 1_048_576))
            .wrap_fn(|request, service| {

                let id = request.headers().get(&REQUEST_ID).cloned();
                let started = Instant::now();
                let pending = service.call(request);

                async move {

                    let mut response = pending.await?;

                    if let Some(id) = id { response.headers_mut().insert(REQUEST_ID, id); }

                    if let Ok(timing) = HeaderValue::from_str(&format!("app;dur={:.2}", started.elapsed().as_secs_f64() * 1_000.0)) { response.headers_mut().insert(SERVER_TIMING, timing); }

                    Ok(response)

                }

            })
            .configure(Routes::mount)

    }).workers(settings.workers).backlog(4_096).bind(settings.listen)?.run().await

}
