use std::collections::HashMap;
use std::net::SocketAddr;

use bytes::Bytes;
use serde::{Deserialize, Serialize};
use sqlx::PgPool;

pub const PAGE_MAX: i64 = 100;
pub const BLOB_MAX: usize = 4_194_304;
pub const TOKEN_TTL: u64 = 86_400;

pub struct Settings {
    pub listen   : SocketAddr,
    pub database : String,
    pub workers  : usize,
    pub pool     : u32,
    pub secret   : Vec<u8>,
    pub tokens   : usize,
}

pub struct State {
    pub pool    : PgPool,
    pub tenants : HashMap<String, i32>,
    pub secret  : Vec<u8>,
    pub blob    : Bytes,
}

#[derive(Clone, Copy)]
pub struct Tenant(pub i32);

#[derive(Clone, Copy)]
pub struct Session {
    pub user   : i64,
    pub tenant : i32,
}

#[derive(Debug)]
pub enum ApiError {
    Tenant,
    Unauthorized,
    NotFound,
    Invalid(&'static str),
    Conflict(&'static str),
    Flaky,
    Database(sqlx::Error),
}

#[derive(Serialize, sqlx::FromRow)]
pub struct Card {
    pub id          : i64,
    pub title       : String,
    pub price_cents : i32,
    pub stock       : i32,
    pub rating      : f32,
    pub category    : String,
}

#[derive(Serialize, sqlx::FromRow)]
pub struct Item {
    pub id          : i64,
    pub title       : String,
    pub description : String,
    pub price_cents : i32,
    pub stock       : i32,
    pub rating      : f32,
    pub category    : String,
    pub created     : i64,
}

#[derive(Serialize, sqlx::FromRow)]
pub struct Comment {
    pub id      : i64,
    pub author  : String,
    pub body    : String,
    pub created : i64,
}

#[derive(Serialize, sqlx::FromRow)]
pub struct Profile {
    pub id     : i64,
    pub email  : String,
    pub name   : String,
    pub orders : i64,
}

#[derive(Serialize, sqlx::FromRow)]
pub struct Order {
    pub id          : i64,
    pub total_cents : i64,
    pub status      : String,
    pub created     : i64,
}

#[derive(Serialize, sqlx::FromRow)]
pub struct Line {
    pub item        : i64,
    pub quantity    : i32,
    pub price_cents : i32,
}

#[derive(Serialize)]
pub struct Page <T> {
    pub page  : i64,
    pub size  : i64,
    pub total : i64,
    pub items : Vec<T>,
}

#[derive(Serialize)]
pub struct Detail {
    #[serde(flatten)]
    pub item     : Item,
    pub comments : Vec<Comment>,
}

#[derive(Serialize)]
pub struct Receipt {
    #[serde(flatten)]
    pub order : Order,
    pub lines : Vec<Line>,
}

#[derive(Deserialize)]
pub struct Listing {
    pub page     : Option<i64>,
    pub size     : Option<i64>,
    pub category : Option<i32>,
    pub q        : Option<String>,
}

#[derive(Deserialize)]
pub struct Login {
    pub email    : String,
    pub password : String,
}

#[derive(Deserialize)]
pub struct Wanted {
    pub item     : i64,
    pub quantity : i32,
}

#[derive(Deserialize)]
pub struct Basket {
    pub lines : Vec<Wanted>,
}

#[derive(Deserialize)]
pub struct Remark {
    pub body : String,
}

#[derive(Deserialize)]
pub struct Shape {
    pub ms      : Option<u64>,
    pub percent : Option<u32>,
    pub bytes   : Option<usize>,
    pub count   : Option<u32>,
}
