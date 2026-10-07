use std::collections::BTreeMap;
use std::io::Read;
use std::path::{Path, PathBuf};

use crate::config::base::consts::{CONFIG_INSTRUCTIONS, CONFIG_MEMORY_BYTES, CONFIG_SOURCE_BYTES, DEFAULT_POOL};
use crate::config::dsl::Dsl;
use crate::config::spec::{Config, HookSpec, Route};
use crate::core::error::{AppError, AppFail, AppResult};
use crate::core::parse::{Lua, Sandbox};

impl Config {

    pub fn load ( path: &Path ) -> AppResult<Self> {

        Self::parse(&Self::source(path)?, &path.to_string_lossy(), path.parent().unwrap_or(Path::new(".")))

    }

    pub fn source ( path: &Path ) -> AppResult<String> {

        let mut source = String::new();

        std::fs::File::open(path).or_fail_with(|| format!("cannot open configuration {}", path.display()))?
            .take(CONFIG_SOURCE_BYTES as u64 + 1).read_to_string(&mut source).or_fail_with(|| format!("cannot read configuration {}", path.display()))?;

        if source.len() > CONFIG_SOURCE_BYTES { return Err(AppError::config("file", format!("configuration {} exceeds {CONFIG_SOURCE_BYTES} bytes", path.display()))); }

        Ok(source)

    }

    pub fn sources ( path: &Path ) -> AppResult<Vec<( String, String )>> {

        if !path.is_dir() { return Ok(vec![( path.to_string_lossy().into_owned(), Self::source(path)? )]); }

        let mut files: Vec<PathBuf> = std::fs::read_dir(path).or_fail_with(|| format!("cannot list configuration directory {}", path.display()))?
            .filter_map(|entry| entry.ok().map(|entry| entry.path()))
            .filter(|file| file.is_file() && file.extension().is_some_and(|extension| extension == "lua"))
            .collect();

        files.sort();

        files.iter().map(|file| Ok(( file.to_string_lossy().into_owned(), Self::source(file)? ))).collect()

    }

    pub fn parse ( source: &str, name: &str, base: &Path ) -> AppResult<Self> {

        let limits = Sandbox { source_bytes: CONFIG_SOURCE_BYTES, memory_bytes: CONFIG_MEMORY_BYTES, instructions: CONFIG_INSTRUCTIONS };
        let lua = Lua::sandbox(limits)?;
        let globals = Dsl::register(&lua, Self::default(), base).or_fail("cannot register configuration functions")?;

        Lua::run(&lua, source, name, globals)?;

        let mut config = Dsl::finish(&lua).ok_or_else(|| AppError::config("file", "configuration draft was lost"))?;

        if config.hooks.request || config.hooks.response { config.hooks = HookSpec { source: source.to_string(), name: name.to_string(), base: base.to_path_buf(), ..config.hooks }; }

        config.resolve(base);
        config.normalize()?;
        config.validate()?;

        Ok(config)

    }

    fn resolve ( &mut self, base: &Path ) {

        let resolve = |value: &mut PathBuf| { if !value.as_os_str().is_empty() && value.is_relative() { *value = base.join(&*value); } };

        if let Some(tls) = &mut self.tls {

            resolve(&mut tls.cert);
            resolve(&mut tls.key);
            resolve(&mut tls.ca_dir);

            for certificate in &mut tls.certificates { resolve(&mut certificate.cert); resolve(&mut certificate.key); }

        }

        if let Some(dir) = &mut self.control.panel_dir { resolve(dir); }

        if let Some(dir) = &mut self.cache.path { resolve(dir); }

        if let Some(hook) = self.tls.as_mut().and_then(|tls| tls.acme.as_mut()).and_then(|acme| acme.dns_hook.as_mut()) { resolve(hook); }

        resolve(&mut self.decisions.path);
        resolve(&mut self.limits.spool_dir);
        if self.access.path.as_os_str() != "stdout" && !self.access.path.to_string_lossy().contains("://") { resolve(&mut self.access.path); }

        for jwt in self.jwts.values_mut() { if let Some(file) = &mut jwt.jwks { resolve(file); } }

        for route in &mut self.routes { if let Some(root) = &mut route.root { resolve(root); } }

        for recipe in self.variables.values_mut() { if let Some(path) = &mut recipe.path { resolve(path); } }

        for spec in self.models.values_mut() {

            resolve(&mut spec.dir);

            if let Some(features) = &mut spec.features { resolve(features); }

        }

        for pool in self.pools.values_mut() {

            for backend in &mut pool.backends {

                for file in [&mut backend.ca_file, &mut backend.cert, &mut backend.key].into_iter().flatten() { resolve(file); }

            }

        }

    }

    pub fn normalize ( &mut self ) -> AppResult<()> {

        if self.default_pool.is_none() && self.pools.contains_key(DEFAULT_POOL) { self.default_pool = Some(DEFAULT_POOL.to_string()); }

        if self.routes.is_empty() && let Some(pool) = &self.default_pool {

            self.routes.push(Route { name: DEFAULT_POOL.to_string(), upstream: pool.clone(), ..Route::default() });

        }

        Self::lowercase(&mut self.request_headers, "set_headers")?;
        Self::lowercase(&mut self.response_headers, "set_headers")?;

        for route in &mut self.routes {

            if route.upstream.is_empty() && !route.deny { route.upstream = self.default_pool.clone().unwrap_or_default(); }

            if let Some(host) = &mut route.host { *host = host.trim_end_matches('.').to_ascii_lowercase(); }

            for method in &mut route.methods { *method = method.to_ascii_uppercase(); }

            Self::lowercase(&mut route.match_headers, "add_route.match_headers")?;
            Self::lowercase(&mut route.request_headers, "add_route.request_headers")?;
            Self::lowercase(&mut route.response_headers, "add_route.response_headers")?;

        }

        Ok(())

    }

    fn lowercase ( headers: &mut BTreeMap<String, String>, key: &str ) -> AppResult<()> {

        let mut normalized = BTreeMap::new();

        for ( name, value ) in std::mem::take(headers) {

            if normalized.insert(name.to_ascii_lowercase(), value).is_some() {

                return Err(AppError::config(key, format!("header `{name}` is listed twice with different casing")));

            }

        }

        *headers = normalized;

        Ok(())

    }

}
