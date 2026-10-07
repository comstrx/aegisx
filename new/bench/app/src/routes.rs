use std::time::Duration;

use actix_web::http::header::{CACHE_CONTROL, CONTENT_TYPE};
use actix_web::{HttpResponse, web};
use bytes::Bytes;
use futures_util::StreamExt;
use futures_util::stream::unfold;
use sha2::{Digest, Sha256};

use crate::arch::{ApiError, BLOB_MAX, Basket, Detail, Listing, Login, PAGE_MAX, Page, Receipt, Remark, Session, Shape, State, Tenant};

pub struct Routes;

type Reply = Result<HttpResponse, ApiError>;

impl Routes {

    pub fn mount ( config: &mut web::ServiceConfig ) {

        config
            .route("/health", web::get().to(Self::health))
            .route("/ready", web::get().to(Self::ready))
            .service(web::scope("/api")
                .route("/catalog", web::get().to(Self::catalog))
                .route("/catalog/{id}", web::get().to(Self::item))
                .route("/catalog/{id}/comments", web::post().to(Self::remark))
                .route("/auth/token", web::post().to(Self::token))
                .route("/me", web::get().to(Self::me))
                .route("/me/favorites", web::get().to(Self::favorites))
                .route("/me/favorites/{id}", web::put().to(Self::favor))
                .route("/me/favorites/{id}", web::delete().to(Self::unfavor))
                .route("/orders", web::post().to(Self::purchase))
                .route("/orders/{id}", web::get().to(Self::receipt))
                .route("/files", web::post().to(Self::upload))
                .route("/events", web::get().to(Self::events))
                .route("/blob", web::get().to(Self::blob))
                .route("/slow", web::get().to(Self::slow))
                .route("/flaky", web::get().to(Self::flaky)));

    }

    async fn health () -> HttpResponse {

        HttpResponse::Ok().body("ok")

    }

    async fn ready ( state: web::Data<State> ) -> Reply {

        state.ready().await?;

        Ok(HttpResponse::Ok().body("ready"))

    }

    async fn catalog ( state: web::Data<State>, tenant: Tenant, query: web::Query<Listing> ) -> Reply {

        let page = query.page.unwrap_or(1).max(1);
        let size = query.size.unwrap_or(20).clamp(1, PAGE_MAX);
        let like = query.q.as_deref().filter(|text| !text.is_empty()).map(|text| format!("%{text}%"));
        let ( total, items ) = state.catalog(tenant.0, query.category, like.as_deref(), size, (page - 1) * size).await?;

        Ok(HttpResponse::Ok().insert_header(( CACHE_CONTROL, "public, max-age=5" )).json(Page { page, size, total, items }))

    }

    async fn item ( state: web::Data<State>, tenant: Tenant, id: web::Path<i64> ) -> Reply {

        let ( item, comments ) = state.item(tenant.0, *id).await?;

        Ok(HttpResponse::Ok().json(Detail { item, comments }))

    }

    async fn remark ( state: web::Data<State>, session: Session, id: web::Path<i64>, remark: web::Json<Remark> ) -> Reply {

        if remark.body.is_empty() || remark.body.len() > 2_000 { return Err(ApiError::Invalid("comment must be 1 to 2000 bytes")); }

        let id = state.remark(session.user, session.tenant, *id, &remark.body).await?;

        Ok(HttpResponse::Created().json(serde_json::json!({ "id": id })))

    }

    async fn token ( state: web::Data<State>, tenant: Tenant, login: web::Json<Login> ) -> Reply {

        let slug = state.tenants.iter().find(|( _, id )| **id == tenant.0).map(|( slug, _ )| slug.as_str()).ok_or(ApiError::Tenant)?;
        let user = state.login(tenant.0, &login.email, &State::digest(slug, &login.password)).await?;
        let expires = State::lifetime();

        Ok(HttpResponse::Ok().insert_header(( CACHE_CONTROL, "no-store" )).json(serde_json::json!({ "token": state.issue(user, tenant.0, expires), "expires": expires })))

    }

    async fn me ( state: web::Data<State>, session: Session ) -> Reply {

        Ok(HttpResponse::Ok().insert_header(( CACHE_CONTROL, "private, no-store" )).json(state.profile(session.user).await?))

    }

    async fn favorites ( state: web::Data<State>, session: Session ) -> Reply {

        Ok(HttpResponse::Ok().insert_header(( CACHE_CONTROL, "private, no-store" )).json(state.favorites(session.user).await?))

    }

    async fn favor ( state: web::Data<State>, session: Session, id: web::Path<i64> ) -> Reply {

        state.favor(session.user, session.tenant, *id, true).await?;

        Ok(HttpResponse::NoContent().finish())

    }

    async fn unfavor ( state: web::Data<State>, session: Session, id: web::Path<i64> ) -> Reply {

        state.favor(session.user, session.tenant, *id, false).await?;

        Ok(HttpResponse::NoContent().finish())

    }

    async fn purchase ( state: web::Data<State>, session: Session, basket: web::Json<Basket> ) -> Reply {

        let basket = basket.into_inner();

        if basket.lines.is_empty() || basket.lines.len() > 20 || basket.lines.iter().any(|line| !(1..=100).contains(&line.quantity)) { return Err(ApiError::Invalid("an order takes 1 to 20 lines of 1 to 100 units")); }

        let ( order, lines ) = state.purchase(session.user, session.tenant, basket.lines).await?;

        Ok(HttpResponse::Created().json(Receipt { order, lines }))

    }

    async fn receipt ( state: web::Data<State>, session: Session, id: web::Path<i64> ) -> Reply {

        let ( order, lines ) = state.receipt(session.user, *id).await?;

        Ok(HttpResponse::Ok().insert_header(( CACHE_CONTROL, "private, no-store" )).json(Receipt { order, lines }))

    }

    async fn upload ( _session: Session, mut payload: web::Payload ) -> Reply {

        let mut digest = Sha256::new();
        let mut bytes = 0usize;

        while let Some(chunk) = payload.next().await {

            let chunk = chunk.map_err(|_| ApiError::Invalid("upload was cut"))?;

            bytes += chunk.len();
            digest.update(&chunk);

        }

        Ok(HttpResponse::Created().json(serde_json::json!({ "bytes": bytes, "sha256": hex::encode(digest.finalize()) })))

    }

    async fn events ( shape: web::Query<Shape> ) -> HttpResponse {

        let count = shape.count.unwrap_or(5).min(1_000);
        let gap = Duration::from_millis(shape.ms.unwrap_or(100).min(10_000));

        let stream = unfold(0u32, move |sent| async move {

            if sent >= count { return None; }

            if sent > 0 { actix_web::rt::time::sleep(gap).await; }

            Some(( Ok::<Bytes, actix_web::Error>(Bytes::from(format!("id: {sent}\ndata: {{\"tick\":{sent}}}\n\n"))), sent + 1 ))

        });

        HttpResponse::Ok().insert_header(( CONTENT_TYPE, "text/event-stream" )).insert_header(( CACHE_CONTROL, "no-cache" )).streaming(stream)

    }

    async fn blob ( state: web::Data<State>, shape: web::Query<Shape> ) -> HttpResponse {

        HttpResponse::Ok().insert_header(( CONTENT_TYPE, "application/octet-stream" )).body(state.blob.slice(..shape.bytes.unwrap_or(65_536).min(BLOB_MAX)))

    }

    async fn slow ( shape: web::Query<Shape> ) -> HttpResponse {

        actix_web::rt::time::sleep(Duration::from_millis(shape.ms.unwrap_or(100).min(60_000))).await;

        HttpResponse::Ok().body("slow")

    }

    async fn flaky ( shape: web::Query<Shape> ) -> Reply {

        let roll = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |since| since.subsec_micros() % 100);

        if roll < shape.percent.unwrap_or(50).min(100) { return Err(ApiError::Flaky); }

        Ok(HttpResponse::Ok().body("fine"))

    }

}
