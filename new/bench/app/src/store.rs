use std::collections::HashMap;

use bytes::Bytes;
use sqlx::postgres::PgPoolOptions;

use crate::arch::{ApiError, BLOB_MAX, Card, Comment, Item, Line, Order, Profile, Settings, State, Wanted};

const SCHEMA: &str = r#"
create table if not exists tenants (id serial primary key, slug text not null unique, name text not null);
create table if not exists users (id bigserial primary key, tenant_id int not null references tenants(id), email text not null, secret text not null, name text not null, unique (tenant_id, email));
create table if not exists categories (id serial primary key, tenant_id int not null references tenants(id), name text not null);
create table if not exists items (id bigserial primary key, tenant_id int not null references tenants(id), category_id int not null references categories(id), title text not null, description text not null, price_cents int not null, stock int not null, rating real not null, created_at timestamptz not null default now());
create table if not exists favorites (user_id bigint not null references users(id), item_id bigint not null references items(id), primary key (user_id, item_id));
create table if not exists orders (id bigserial primary key, tenant_id int not null references tenants(id), user_id bigint not null references users(id), total_cents bigint not null, status text not null, created_at timestamptz not null default now());
create table if not exists order_lines (order_id bigint not null references orders(id), item_id bigint not null references items(id), quantity int not null, price_cents int not null);
create table if not exists comments (id bigserial primary key, item_id bigint not null references items(id), user_id bigint not null references users(id), body text not null, created_at timestamptz not null default now());
create index if not exists items_tenant on items (tenant_id, id desc);
create index if not exists items_category on items (tenant_id, category_id, id desc);
create index if not exists comments_item on comments (item_id, id desc);
create index if not exists orders_user on orders (user_id, id desc);
create index if not exists lines_order on order_lines (order_id);
"#;

const SEED: &str = r#"
insert into tenants (slug, name) select 't' || n, 'Tenant ' || n from generate_series(1, 4) n;
insert into users (tenant_id, email, secret, name) select t.id, 'user' || n || '@' || t.slug || '.test', encode(sha256(convert_to(t.slug || ':secret', 'UTF8')), 'hex'), 'User ' || n from tenants t, generate_series(1, 200) n;
insert into categories (tenant_id, name) select t.id, 'Category ' || n from tenants t, generate_series(1, 8) n;
insert into items (tenant_id, category_id, title, description, price_cents, stock, rating) select c.tenant_id, c.id, 'Item ' || n || ' of ' || c.name, repeat('A dependable product with a description long enough to look like real catalog copy. ', 4), 500 + (n * 37) % 90000, 1000000, 1 + (n % 40) / 10.0 from categories c, generate_series(1, 320) n;
insert into comments (item_id, user_id, body) select i.id, u.id, 'Works as described, comment ' || n from items i join lateral (select id from users where tenant_id = i.tenant_id order by id limit 1) u on true, generate_series(1, 3) n where i.id % 3 = 0;
analyze;
"#;

impl State {

    pub async fn open ( settings: &Settings ) -> Result<Self, sqlx::Error> {

        let pool = PgPoolOptions::new().max_connections(settings.pool).min_connections(settings.pool.min(4)).connect(&settings.database).await?;

        sqlx::raw_sql(SCHEMA).execute(&pool).await?;

        if sqlx::query_scalar::<_, i64>("select count(*) from tenants").fetch_one(&pool).await? == 0 { sqlx::raw_sql(SEED).execute(&pool).await?; }

        let tenants: HashMap<String, i32> = sqlx::query_as::<_, ( String, i32 )>("select slug, id from tenants").fetch_all(&pool).await?.into_iter().collect();
        let blob = Bytes::from((0..BLOB_MAX).map(|index| b'a' + (index % 26) as u8).collect::<Vec<u8>>());

        Ok(Self { pool, tenants, secret: settings.secret.clone(), blob })

    }

    pub async fn ready ( &self ) -> Result<(), ApiError> {

        sqlx::query_scalar::<_, i32>("select 1").fetch_one(&self.pool).await.map(|_| ()).map_err(ApiError::Database)

    }

    pub async fn catalog ( &self, tenant: i32, category: Option<i32>, like: Option<&str>, size: i64, offset: i64 ) -> Result<( i64, Vec<Card> ), ApiError> {

        let total = sqlx::query_scalar::<_, i64>("select count(*) from items where tenant_id = $1 and ($2::int is null or category_id = $2) and ($3::text is null or title ilike $3)")
            .bind(tenant).bind(category).bind(like).fetch_one(&self.pool).await.map_err(ApiError::Database)?;

        let cards = sqlx::query_as::<_, Card>("select i.id, i.title, i.price_cents, i.stock, i.rating, c.name as category from items i join categories c on c.id = i.category_id where i.tenant_id = $1 and ($2::int is null or i.category_id = $2) and ($3::text is null or i.title ilike $3) order by i.id desc limit $4 offset $5")
            .bind(tenant).bind(category).bind(like).bind(size).bind(offset).fetch_all(&self.pool).await.map_err(ApiError::Database)?;

        Ok(( total, cards ))

    }

    pub async fn item ( &self, tenant: i32, id: i64 ) -> Result<( Item, Vec<Comment> ), ApiError> {

        let item = sqlx::query_as::<_, Item>("select i.id, i.title, i.description, i.price_cents, i.stock, i.rating, c.name as category, extract(epoch from i.created_at)::bigint as created from items i join categories c on c.id = i.category_id where i.tenant_id = $1 and i.id = $2")
            .bind(tenant).bind(id).fetch_optional(&self.pool).await.map_err(ApiError::Database)?.ok_or(ApiError::NotFound)?;

        let comments = sqlx::query_as::<_, Comment>("select m.id, u.name as author, m.body, extract(epoch from m.created_at)::bigint as created from comments m join users u on u.id = m.user_id where m.item_id = $1 order by m.id desc limit 5")
            .bind(id).fetch_all(&self.pool).await.map_err(ApiError::Database)?;

        Ok(( item, comments ))

    }

    pub async fn login ( &self, tenant: i32, email: &str, secret: &str ) -> Result<i64, ApiError> {

        sqlx::query_scalar::<_, i64>("select id from users where tenant_id = $1 and email = $2 and secret = $3")
            .bind(tenant).bind(email).bind(secret).fetch_optional(&self.pool).await.map_err(ApiError::Database)?.ok_or(ApiError::Unauthorized)

    }

    pub async fn members ( &self, limit: i64 ) -> Result<Vec<( i64, i32 )>, sqlx::Error> {

        sqlx::query_as::<_, ( i64, i32 )>("select id, tenant_id from users order by id limit $1").bind(limit).fetch_all(&self.pool).await

    }

    pub async fn profile ( &self, user: i64 ) -> Result<Profile, ApiError> {

        sqlx::query_as::<_, Profile>("select u.id, u.email, u.name, (select count(*) from orders o where o.user_id = u.id) as orders from users u where u.id = $1")
            .bind(user).fetch_optional(&self.pool).await.map_err(ApiError::Database)?.ok_or(ApiError::Unauthorized)

    }

    pub async fn favorites ( &self, user: i64 ) -> Result<Vec<Card>, ApiError> {

        sqlx::query_as::<_, Card>("select i.id, i.title, i.price_cents, i.stock, i.rating, c.name as category from favorites f join items i on i.id = f.item_id join categories c on c.id = i.category_id where f.user_id = $1 order by i.id desc limit 50")
            .bind(user).fetch_all(&self.pool).await.map_err(ApiError::Database)

    }

    pub async fn favor ( &self, user: i64, tenant: i32, item: i64, keep: bool ) -> Result<(), ApiError> {

        let done = match keep {
            true => sqlx::query("insert into favorites (user_id, item_id) select $1, id from items where id = $2 and tenant_id = $3 on conflict do nothing").bind(user).bind(item).bind(tenant).execute(&self.pool).await,
            false => sqlx::query("delete from favorites where user_id = $1 and item_id = $2").bind(user).bind(item).execute(&self.pool).await,
        };

        done.map(|_| ()).map_err(ApiError::Database)

    }

    pub async fn remark ( &self, user: i64, tenant: i32, item: i64, body: &str ) -> Result<i64, ApiError> {

        sqlx::query_scalar::<_, i64>("insert into comments (item_id, user_id, body) select id, $1, $4 from items where id = $2 and tenant_id = $3 returning id")
            .bind(user).bind(item).bind(tenant).bind(body).fetch_optional(&self.pool).await.map_err(ApiError::Database)?.ok_or(ApiError::NotFound)

    }

    pub async fn purchase ( &self, user: i64, tenant: i32, mut wanted: Vec<Wanted> ) -> Result<( Order, Vec<Line> ), ApiError> {

        wanted.sort_by_key(|line| line.item);

        let mut transaction = self.pool.begin().await.map_err(ApiError::Database)?;
        let mut lines = Vec::with_capacity(wanted.len());
        let mut total = 0i64;

        for line in &wanted {

            let price = sqlx::query_scalar::<_, i32>("update items set stock = stock - $1 where id = $2 and tenant_id = $3 and stock >= $1 returning price_cents")
                .bind(line.quantity).bind(line.item).bind(tenant).fetch_optional(&mut *transaction).await.map_err(ApiError::Database)?.ok_or(ApiError::Conflict("item is missing or out of stock"))?;

            total += i64::from(price) * i64::from(line.quantity);
            lines.push(Line { item: line.item, quantity: line.quantity, price_cents: price });

        }

        let order = sqlx::query_as::<_, Order>("insert into orders (tenant_id, user_id, total_cents, status) values ($1, $2, $3, 'placed') returning id, total_cents, status, extract(epoch from created_at)::bigint as created")
            .bind(tenant).bind(user).bind(total).fetch_one(&mut *transaction).await.map_err(ApiError::Database)?;

        for line in &lines {

            sqlx::query("insert into order_lines (order_id, item_id, quantity, price_cents) values ($1, $2, $3, $4)")
                .bind(order.id).bind(line.item).bind(line.quantity).bind(line.price_cents).execute(&mut *transaction).await.map_err(ApiError::Database)?;

        }

        transaction.commit().await.map_err(ApiError::Database)?;

        Ok(( order, lines ))

    }

    pub async fn receipt ( &self, user: i64, id: i64 ) -> Result<( Order, Vec<Line> ), ApiError> {

        let order = sqlx::query_as::<_, Order>("select id, total_cents, status, extract(epoch from created_at)::bigint as created from orders where id = $1 and user_id = $2")
            .bind(id).bind(user).fetch_optional(&self.pool).await.map_err(ApiError::Database)?.ok_or(ApiError::NotFound)?;

        let lines = sqlx::query_as::<_, Line>("select item_id as item, quantity, price_cents from order_lines where order_id = $1 order by item_id")
            .bind(id).fetch_all(&self.pool).await.map_err(ApiError::Database)?;

        Ok(( order, lines ))

    }

}
