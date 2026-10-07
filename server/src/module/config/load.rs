use std::cell::RefCell;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use mlua::{HookTriggers, Lua, LuaOptions, StdLib, VmState};

use crate::core::error::{AppError, AppFail, AppResult};
use super::{BackendConfig, Config, Mode};

impl Config {

    pub fn load ( path: &Path ) -> AppResult<Self> {

        let mut source = String::new();
        std::fs::File::open(path).or_fail("Cannot open configuration")?.take(65537)
            .read_to_string(&mut source).or_fail("Cannot read configuration")?;
        if source.len() > 65536 { return Err(AppError::invalid("Configuration exceeds 64 KiB")); }
        let lua = Lua::new_with(StdLib::NONE, LuaOptions::default()).or_fail("Cannot initialize Lua")?;
        lua.set_memory_limit(1048576).or_fail("Cannot limit Lua memory")?;
        lua.set_hook(HookTriggers::new().every_nth_instruction(10000), |_, _| {
            Err::<VmState, _>(mlua::Error::RuntimeError("Configuration instruction budget exceeded".into()))
        }).or_fail("Cannot set Lua instruction budget")?;
        let state = Rc::new(RefCell::new(Self::default()));
        let globals = super::dsl::register(&lua, state.clone()).or_fail("Cannot register Lua DSL")?;
        lua.load(&source).set_name(path.to_string_lossy()).set_environment(globals).exec().or_fail("Invalid Lua configuration")?;
        let mut config = state.borrow().clone();
        let base = path.parent().unwrap_or(Path::new("."));
        let resolve = |value: &mut PathBuf| { if value.is_relative() { *value = base.join(&*value); } };
        if let Some(value) = &mut config.store { resolve(value); }
        if let Some(value) = &mut config.model.directory { resolve(value); }
        if let Some(tls) = &mut config.tls { resolve(&mut tls.cert); resolve(&mut tls.key); }
        if config.default_pool.as_deref() == Some("default") && config.pools.get("default").is_none_or(|pool| pool.backends.is_empty()) {
            config.pools.entry("default".into()).or_default().backends.push(BackendConfig {
                address: config.upstream, ..BackendConfig::default()
            });
        }
        for pool in config.pools.values_mut() {
            for backend in &mut pool.backends {
                if let Some(value) = &mut backend.ca_file { resolve(value); }
            }
        }
        let normalize = |headers: &mut std::collections::BTreeMap<String, String>| -> AppResult<()> {
            let mut normalized = std::collections::BTreeMap::new();
            for (key, value) in std::mem::take(headers) {
                if normalized.insert(key.to_ascii_lowercase(), value).is_some() {
                    return Err(AppError::invalid("Duplicate case-insensitive header"));
                }
            }
            *headers = normalized;
            Ok(())
        };
        normalize(&mut config.request_headers)?;
        normalize(&mut config.response_headers)?;
        for route in &mut config.routes {
            if route.upstream.is_empty() { route.upstream = config.default_pool.clone().unwrap_or_default(); }
            normalize(&mut route.match_headers)?;
            normalize(&mut route.request_headers)?;
            normalize(&mut route.response_headers)?;
            if let Some(host) = &mut route.host { *host = host.trim_end_matches('.').to_ascii_lowercase(); }
        }
        config.validate()?;

        Ok(config)

    }

    pub fn needs_model ( &self ) -> bool {

        self.model.mode != Mode::Off || self.routes.iter().any(|route| route.model.is_some_and(|mode| mode != Mode::Off))

    }

    pub fn needs_enforcement ( &self ) -> bool {

        self.routes.iter().any(|route|route.cancellation && route.model.unwrap_or(self.model.mode)==Mode::Background)
            || self.model.mode == Mode::Enforce || self.routes.iter().any(|route| route.model == Some(Mode::Enforce))
            || (self.cache.decisions && self.cache.background_denials && (self.model.mode == Mode::Background
                || self.routes.iter().any(|route| route.model == Some(Mode::Background))))

    }

}
