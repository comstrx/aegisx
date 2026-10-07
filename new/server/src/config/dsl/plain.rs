use std::collections::BTreeMap;

use ipnet::IpNet;
use mlua::{Table, Value};

use crate::config::spec::{BackendConfig, Balance, BasicAuth, Config, HealthConfig, Respond, Route};
use crate::core::parse::Lua;
use crate::http::upstream::Protocol;
use super::arch::Dsl;

const TIMES: [( &str, u64 ); 5] = [( "", 1 ), ( "ms", 1 ), ( "s", 1_000 ), ( "m", 60_000 ), ( "h", 3_600_000 )];
const SIZES: [( &str, u64 ); 5] = [( "", 1 ), ( "b", 1 ), ( "kb", 1_024 ), ( "mb", 1_048_576 ), ( "gb", 1_073_741_824 )];
const SHORT: [&str; 12] = ["to", "files", "spa", "respond", "strip", "limit", "burst", "timeout", "max_body", "allow", "deny", "auth"];

impl Dsl {

    pub(super) fn plain ( lua: &mlua::Lua, globals: &Table ) -> mlua::Result<()> {

        globals.set("listen", globals.get::<Value>("set_listen")?)?;

        globals.set("upstream", lua.create_function(|lua, name: String| {

            lua.create_function(move |lua, options: Table| Self::upstream(lua, &name, options))

        })?)?;

        globals.set("route", lua.create_function(|lua, path: String| {

            lua.create_function(move |lua, options: Table| Self::route(lua, &path, options))

        })?)?;

        globals.set("site", lua.create_function(|lua, host: String| {

            lua.create_function(move |lua, routes: Table| Self::site(lua, &host, routes))

        })?)?;

        Ok(())

    }

    fn upstream ( lua: &mlua::Lua, name: &str, options: Table ) -> mlua::Result<()> {

        let mut backends = Vec::new();
        let ( mut balance, mut check, mut keepalive, mut slow, mut fastcgi ) = ( None::<Balance>, None::<String>, None::<usize>, None::<u64>, false );

        for pair in options.pairs::<Value, Value>() {

            let ( key, value ) = pair?;

            match key {
                Value::Integer(_) => backends.push(match &value {
                    Value::String(_) => BackendConfig { address: Self::target("upstream", &value)?, ..BackendConfig::default() },
                    _ => Lua::table::<BackendConfig>(lua, "upstream", value)?,
                }),
                Value::String(key) => match &*key.to_str()? {
                    "balance" => balance = Some(Lua::value(lua, "upstream balance", value)?),
                    "check" => check = Some(Lua::text("upstream check", &value)?),
                    "keepalive" => keepalive = Some(Lua::value(lua, "upstream keepalive", value)?),
                    "slow" => slow = Some(Self::amount("upstream slow", &value, &TIMES)?),
                    "fastcgi" => fastcgi = matches!(value, Value::Boolean(true)),
                    "srv" => backends.push(BackendConfig { srv: Some(Lua::text("upstream srv", &value)?), ..BackendConfig::default() }),
                    other => return Err(Lua::fail(format!("upstream `{name}`: unknown option `{other}`; it takes addresses, balance, check, keepalive, slow, fastcgi and srv"))),
                },
                _ => return Err(Lua::fail(format!("upstream `{name}` takes addresses and named options"))),
            }

        }

        if backends.is_empty() { return Err(Lua::fail(format!("upstream `{name}` needs at least one address"))); }

        Self::draft(lua, |config| {

            let pool = config.pools.entry(name.to_string()).or_default();

            for mut backend in backends {

                if fastcgi { backend.protocol = Protocol::Fastcgi; }

                pool.backends.push(backend);

            }

            if let Some(balance) = balance { pool.options.policy = balance; }

            if let Some(path) = check { pool.options.health = Some(HealthConfig { path: Some(path), ..HealthConfig::default() }); }

            if let Some(keepalive) = keepalive { pool.options.keepalive = keepalive; }

            if let Some(slow) = slow { pool.options.slow_ms = slow; }

        })

    }

    fn route ( lua: &mlua::Lua, path: &str, options: Table ) -> mlua::Result<Table> {

        let rest = lua.create_table()?;
        let mut short: Vec<( String, Value )> = Vec::new();

        for pair in options.pairs::<String, Value>() {

            let ( key, value ) = pair?;

            match SHORT.contains(&key.as_str()) { true => short.push(( key, value )), false => rest.set(key, value)? }

        }

        rest.set("path", path)?;

        if !rest.contains_key("name")? { rest.set("name", path)?; }

        let mut route = Lua::table::<Route>(lua, "route", Value::Table(rest))?;

        for ( key, value ) in short {

            match key.as_str() {
                "to" => route.upstream = Lua::text("route to", &value)?,
                "files" => route.root = Some(Lua::text("route files", &value)?.into()),
                "spa" => { if matches!(value, Value::Boolean(true)) { route.try_files = vec!["$uri".to_string(), "/index.html".to_string()]; } }
                "respond" => route.respond = Some(match &value {
                    Value::Integer(status) => Respond { status: u16::try_from(*status).map_err(|_| Lua::fail("route respond: the status is out of range"))?, ..Respond::default() },
                    Value::String(_) => Respond { body: Lua::text("route respond", &value)?, ..Respond::default() },
                    _ => Lua::table::<Respond>(lua, "route respond", value)?,
                }),
                "strip" => route.strip_prefix = matches!(value, Value::Boolean(true)),
                "limit" => route.rate_per_second = Some(Self::rate("route limit", &value)?),
                "burst" => route.rate_burst = Some(Lua::value(lua, "route burst", value)?),
                "timeout" => route.timeout_ms = Some(Self::amount("route timeout", &value, &TIMES)?),
                "max_body" => route.max_body_bytes = Some(usize::try_from(Self::amount("route max_body", &value, &SIZES)?).unwrap_or(usize::MAX)),
                "allow" => route.acl.allow = Self::nets(lua, "route allow", value)?,
                "deny" => route.acl.deny = Self::nets(lua, "route deny", value)?,
                "auth" => route.basic_auth = Some(BasicAuth { users: Lua::value::<BTreeMap<String, String>>(lua, "route auth", value)?, ..BasicAuth::default() }),
                _ => {}
            }

        }

        let mut index = 0usize;

        Self::draft(lua, |config: &mut Config| { index = config.routes.len(); config.routes.push(route); })?;

        let handle = lua.create_table()?;

        handle.set("route", index)?;

        Ok(handle)

    }

    fn site ( lua: &mlua::Lua, host: &str, routes: Table ) -> mlua::Result<()> {

        let mut indexes = Vec::new();

        for pair in routes.pairs::<Value, Value>() {

            match pair? {
                ( Value::Integer(_), Value::Table(handle) ) => indexes.push(handle.get::<usize>("route").map_err(|_| Lua::fail(format!("site `{host}` takes route \"/path\" {{ ... }} entries")))?),
                _ => return Err(Lua::fail(format!("site `{host}` takes route \"/path\" {{ ... }} entries separated by commas"))),
            }

        }

        Self::draft(lua, |config| {

            for index in indexes {

                let Some(route) = config.routes.get_mut(index) else { continue; };

                if route.name == route.path { route.name = format!("{host}{}", route.path); }

                route.host = Some(host.to_string());

            }

        })

    }

    fn nets ( lua: &mlua::Lua, what: &str, value: Value ) -> mlua::Result<Vec<IpNet>> {

        Lua::value::<Vec<String>>(lua, what, value)?.iter().map(|text| text.parse::<IpNet>().or_else(|_| text.parse::<std::net::IpAddr>().map(IpNet::from)).map_err(|_| Lua::fail(format!("{what}: `{text}` is not an address or a network")))).collect()

    }

    fn amount ( what: &str, value: &Value, units: &[( &str, u64 )] ) -> mlua::Result<u64> {

        match value {
            Value::Integer(number) => u64::try_from(*number).map_err(|_| Lua::fail(format!("{what} cannot be negative"))),
            Value::String(text) => {

                let text = text.to_str()?;
                let text = text.trim();
                let split = text.find(|letter: char| !letter.is_ascii_digit()).unwrap_or(text.len());
                let number = text[..split].parse::<u64>().map_err(|_| Lua::fail(format!("{what}: `{text}` does not start with a number")))?;
                let unit = text[split..].trim();

                units.iter().find(|( name, _ )| name.eq_ignore_ascii_case(unit)).map(|( _, scale )| number.saturating_mul(*scale)).ok_or_else(|| Lua::fail(format!("{what}: `{text}` ends in an unknown unit; use {}", units.iter().map(|( name, _ )| *name).filter(|name| !name.is_empty()).collect::<Vec<_>>().join(", "))))

            }
            other => Err(Lua::fail(format!("{what} takes a number or text such as `30s`, found {}", other.type_name()))),
        }

    }

    fn rate ( what: &str, value: &Value ) -> mlua::Result<u32> {

        let fail = || Lua::fail(format!("{what} takes a number per second or text such as `10/s` or `600/m`"));

        match value {
            Value::Integer(number) => u32::try_from(*number).map_err(|_| fail()),
            Value::String(text) => {

                let text = text.to_str()?;
                let ( count, per ) = text.trim().split_once('/').ok_or_else(fail)?;
                let count = count.trim().parse::<u32>().map_err(|_| fail())?;

                match per.trim() {
                    "s" | "sec" | "second" => Ok(count),
                    "m" | "min" | "minute" => Ok(count.div_ceil(60).max(1)),
                    _ => Err(fail()),
                }

            }
            _ => Err(fail()),
        }

    }

}
