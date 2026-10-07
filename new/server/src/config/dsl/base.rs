use std::cell::Cell;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use mlua::{Table, Value};

use crate::config::spec::{BackendConfig, Balance, Certificate, Config, JwtConfig, ListenConfig, ModelSpec, PoolOptions, Route, StickyConfig, TlsConfig};
use crate::config::base::consts::{CONFIG_INCLUDE_DEPTH, CONFIG_LIBRARY};
use crate::core::env::Env;
use crate::http::variable::{Kind, Recipe};
use crate::core::net::{Addr, Address};
use crate::config::StreamConfig;
use crate::core::parse::Lua;
use crate::http::upstream::Protocol;
use super::arch::{Dsl, SETTERS, Site};

impl Dsl {

    pub fn register ( lua: &mlua::Lua, config: Config, base: &Path ) -> mlua::Result<Table> {

        lua.set_app_data(config);

        let globals = lua.create_table()?;

        Lua::share(lua, &globals, &CONFIG_LIBRARY)?;

        globals.set("env", lua.create_function(|_, name: String| Ok(Env::get(name)))?)?;

        let ( root, scope, depth ) = ( base.to_path_buf(), globals.clone(), Rc::new(Cell::new(0usize)) );

        globals.set("include", lua.create_function(move |lua, path: String| {

            if depth.get() >= CONFIG_INCLUDE_DEPTH { return Err(Lua::fail(format!("include `{path}` nests deeper than {CONFIG_INCLUDE_DEPTH} levels"))); }

            let files = Config::sources(&root.join(&path)).map_err(|error| Lua::fail(format!("include `{path}`: {error}")))?;

            depth.set(depth.get() + 1);

            let outcome = files.iter().try_for_each(|( name, source )| lua.load(source.as_str()).set_name(name.as_str()).set_environment(scope.clone()).exec());

            depth.set(depth.get() - 1);

            outcome

        })?)?;

        globals.set("set_listen_unix", lua.create_function(|lua, path: String| {

            Self::draft(lua, |config| config.listen_unix = Some(path).filter(|path| !path.is_empty()))

        })?)?;

        globals.set("set_listen", lua.create_function(|lua, value: Value| {

            let address = Self::address("set_listen", &value)?;

            Self::draft(lua, |config| config.listen = address)

        })?)?;

        globals.set("set_upstream", lua.create_function(|lua, value: Value| {

            let address = Self::address("set_upstream", &value)?;

            Self::draft(lua, |config| config.set_upstream(address))

        })?)?;

        globals.set("set_default_upstream", lua.create_function(|lua, value: Value| {

            let name = match value {
                Value::Boolean(false) | Value::Nil => None,
                other => Some(Lua::text("set_default_upstream", &other)?),
            };

            Self::draft(lua, |config| config.default_pool = name)

        })?)?;

        globals.set("add_upstream", lua.create_function(|lua, ( name, value ): ( String, Value )| {

            let backend = match &value {
                Value::String(_) => BackendConfig { address: Self::target("add_upstream", &value)?, ..BackendConfig::default() },
                _ => Lua::table::<BackendConfig>(lua, "add_upstream", value)?,
            };

            Self::draft(lua, |config| config.pools.entry(name).or_default().backends.push(backend))

        })?)?;

        globals.set("add_certificate", lua.create_function(|lua, value: Value| {

            let certificate = Lua::table::<Certificate>(lua, "add_certificate", value)?;

            Self::draft(lua, |config| config.tls.get_or_insert_with(TlsConfig::default).certificates.push(certificate))

        })?)?;

        globals.set("add_model", lua.create_function(|lua, ( name, value ): ( String, Value )| {

            let spec = match &value {
                Value::String(_) => ModelSpec { dir: PathBuf::from(Lua::text("add_model", &value)?), ..ModelSpec::default() },
                _ => Lua::table::<ModelSpec>(lua, "add_model", value)?,
            };

            Self::draft(lua, |config| { config.models.insert(name, spec); })

        })?)?;

        globals.set("set_balancer", lua.create_function(|lua, ( name, value ): ( String, Value )| {

            match &value {
                Value::String(_) => {

                    let policy: Balance = Lua::value(lua, "set_balancer", value)?;

                    Self::draft(lua, |config| config.pools.entry(name).or_default().options.policy = policy)

                }
                _ => {

                    let options = Lua::table::<PoolOptions>(lua, "set_balancer", value)?;

                    Self::draft(lua, |config| config.pools.entry(name).or_default().options = options)

                }
            }

        })?)?;

        globals.set("set_sticky", lua.create_function(|lua, ( name, value ): ( String, Value )| {

            let sticky = Lua::table::<StickyConfig>(lua, "set_sticky", value)?;

            Self::draft(lua, |config| config.pools.entry(name).or_default().options.sticky = Some(sticky))

        })?)?;

        globals.set("add_listen", lua.create_function(|lua, value: Value| {

            let listener = match &value {
                Value::String(_) => ListenConfig { address: Self::address("add_listen", &value)?, ..ListenConfig::default() },
                _ => Lua::table::<ListenConfig>(lua, "add_listen", value)?,
            };

            Self::draft(lua, |config| config.listeners.push(listener))

        })?)?;

        globals.set("add_stream", lua.create_function(|lua, value: Value| {

            let stream = Lua::table::<StreamConfig>(lua, "add_stream", value)?;

            Self::draft(lua, |config| config.streams.push(stream))

        })?)?;

        for ( function, kind ) in [( "add_map", Kind::Map ), ( "add_geo", Kind::Geo ), ( "add_split", Kind::Split ), ( "add_geoip", Kind::Mmdb ), ( "add_keyval", Kind::Keyval )] {

            globals.set(function, lua.create_function(move |lua, ( name, value ): ( String, Value )| {

                let recipe = Recipe { kind, ..Lua::table::<Recipe>(lua, function, value)? };

                Self::draft(lua, |config| { config.variables.insert(name, recipe); })

            })?)?;

        }

        for preset in ["static_site", "spa", "php_app"] {

            globals.set(preset, lua.create_function(move |lua, value: Value| {

                let site = Lua::table::<Site>(lua, preset, value)?;
                let mut outcome = Ok(());

                Self::draft(lua, |config| outcome = site.expand(preset, config))?;

                outcome.map_err(Lua::fail)

            })?)?;

        }

        for function in ["on_request", "on_response"] {

            globals.set(function, lua.create_function(move |lua, hook: mlua::Function| {

                lua.set_named_registry_value(function, hook)?;

                Self::draft(lua, |config| if function == "on_request" { config.hooks.request = true } else { config.hooks.response = true })

            })?)?;

        }

        globals.set("add_jwt", lua.create_function(|lua, ( name, value ): ( String, Value )| {

            let jwt = Lua::table::<JwtConfig>(lua, "add_jwt", value)?;

            Self::draft(lua, |config| { config.jwts.insert(name, jwt); })

        })?)?;

        globals.set("add_route", lua.create_function(|lua, value: Value| {

            let route = Lua::table::<Route>(lua, "add_route", value)?;

            Self::draft(lua, |config| config.routes.push(route))

        })?)?;

        globals.set("set_headers", lua.create_function(|lua, ( direction, value ): ( String, Value )| {

            let headers = Lua::table(lua, "set_headers", value)?;

            match direction.as_str() {
                "request" => Self::draft(lua, |config| config.request_headers = headers),
                "response" => Self::draft(lua, |config| config.response_headers = headers),
                other => Err(Lua::fail(format!("set_headers direction must be request or response, found `{other}`"))),
            }

        })?)?;

        globals.set("set_preserve_host", lua.create_function(|lua, value: bool| {

            Self::draft(lua, |config| config.preserve_host = value)

        })?)?;

        for name in SETTERS {

            globals.set(name, lua.create_function(move |lua, value: Value| Self::group(lua, name, value))?)?;

        }

        Self::plain(lua, &globals)?;

        Ok(globals)

    }

    pub fn finish ( lua: &mlua::Lua ) -> Option<Config> {

        lua.remove_app_data::<Config>()

    }

    fn group ( lua: &mlua::Lua, name: &str, value: Value ) -> mlua::Result<()> {

        match name {
            "set_acl" => { let group = Lua::table(lua, name, value)?; Self::draft(lua, |config| config.acl = group) }
            "set_runtime" => { let group = Lua::table(lua, name, value)?; Self::draft(lua, |config| config.runtime = group) }
            "set_server" => { let group = Lua::table(lua, name, value)?; Self::draft(lua, |config| config.server = group) }
            "set_client" => { let group = Lua::table(lua, name, value)?; Self::draft(lua, |config| config.client = group) }
            "set_limits" => { let group = Lua::table(lua, name, value)?; Self::draft(lua, |config| config.limits = group) }
            "set_identity" => { let group = Lua::table(lua, name, value)?; Self::draft(lua, |config| config.identity = group) }
            "set_log" => { let group = Lua::table(lua, name, value)?; Self::draft(lua, |config| config.log = group) }
            "set_control" => { let group = Lua::table(lua, name, value)?; Self::draft(lua, |config| config.control = group) }
            "set_telemetry" => { let group = Lua::table(lua, name, value)?; Self::draft(lua, |config| config.telemetry = group) }
            "set_analysis" => { let group = Lua::table(lua, name, value)?; Self::draft(lua, |config| config.analysis = group) }
            "set_decisions" => { let group = Lua::table(lua, name, value)?; Self::draft(lua, |config| config.decisions = group) }
            "set_access_log" => { let group = Lua::table(lua, name, value)?; Self::draft(lua, |config| config.access = group) }
            "set_compression" => { let group = Lua::table(lua, name, value)?; Self::draft(lua, |config| config.compression = group) }
            "set_files" => { let group = Lua::table(lua, name, value)?; Self::draft(lua, |config| config.files = group) }
            "set_cache" => { let group = Lua::table(lua, name, value)?; Self::draft(lua, |config| config.cache = group) }
            "set_http3" => { let group = Lua::table(lua, name, value)?; Self::draft(lua, |config| config.http3 = group) }
            "set_error_pages" => { let group = Lua::table(lua, name, value)?; Self::draft(lua, |config| config.error_pages = group) }
            "set_tls" => {

                let group = match value {
                    Value::Boolean(false) | Value::Nil => None,
                    other => Some(Lua::table(lua, name, other)?),
                };

                Self::draft(lua, |config| config.tls = group)

            }
            other => Err(Lua::fail(format!("unknown setter `{other}`"))),
        }

    }

    pub(super) fn draft ( lua: &mlua::Lua, apply: impl FnOnce(&mut Config) ) -> mlua::Result<()> {

        let mut config = lua.app_data_mut::<Config>().ok_or_else(|| Lua::fail("configuration draft is unavailable"))?;

        apply(&mut config);

        Ok(())

    }

    pub(super) fn target ( what: &str, value: &Value ) -> mlua::Result<Address> {

        let text = Lua::text(what, value)?;

        Address::parse(&text).map_err(|error| Lua::fail(format!("{what}: {error}")))

    }

    fn address ( what: &str, value: &Value ) -> mlua::Result<std::net::SocketAddr> {

        let text = Lua::text(what, value)?;

        Addr::parse(&text).map_err(|error| Lua::fail(format!("{what}: {error}")))

    }

}

impl Site {

    fn expand ( self, preset: &str, config: &mut Config ) -> Result<(), String> {

        if self.root.as_os_str().is_empty() { return Err(format!("{preset} needs `root`")); }

        let label = self.host.clone().unwrap_or_else(|| "any".to_string());
        let path = if self.path.is_empty() { "/".to_string() } else { self.path };
        let mut route = Route { name: format!("{preset}:{label}{path}"), host: self.host, path, root: Some(self.root), cache_control: self.cache_control, ..Route::default() };

        match preset {
            "spa" => route.try_files = vec!["$uri".to_string(), "/index.html".to_string()],
            "php_app" => {

                let target = self.fastcgi.ok_or_else(|| "php_app needs `fastcgi`, such as \"unix:/run/php/php-fpm.sock\"".to_string())?;
                let address = Address::parse(&target).map_err(|error| format!("php_app fastcgi: {error}"))?;
                let pool = format!("php:{label}");

                config.pools.entry(pool.clone()).or_default().backends.push(BackendConfig { address, protocol: Protocol::Fastcgi, ..BackendConfig::default() });
                route.upstream = pool;

            }
            _ => {}
        }

        config.routes.push(route);

        Ok(())

    }

}

