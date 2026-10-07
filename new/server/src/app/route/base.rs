use std::collections::BTreeMap;
use std::sync::Arc;

use http::header::HeaderName;
use bytes::Bytes;
use http::Method;

use crate::config::{Config, ForwardAuth, Route};
use crate::core::error::{AppError, AppResult};
use crate::http::header::{Header, Rendered, Template};
use crate::http::files::{Candidate, Files};
use crate::http::proxy::{Edit, Plan};
use crate::http::rewrite::Rewrite;
use crate::http::route::{Pattern, Router};
use crate::app::{Basic, Bearer, HashKey, Hint, Link};
use crate::http::encode::Swaps;
use crate::http::fastcgi::Script;
use crate::http::upstream::Protocol;
use crate::http::variable::{Catalog, Check, Subject};
use super::arch::{Errors, Fence, Policy, Reply, RouteState, Routes, Rule, Table, Verifier};

impl Routes {

    pub fn compile ( config: &Config ) -> AppResult<( Vec<Arc<RouteState>>, Table, Catalog )> {

        let catalog = Catalog::compile(&config.variables)?;
        let bearers = config.jwts.iter().map(|( name, spec )| Ok(( name.as_str(), Arc::new(Bearer::compile(spec).map_err(|error| AppError::config("add_jwt", format!("`{name}`: {error}")))?) ))).collect::<AppResult<BTreeMap<&str, Arc<Bearer>>>>()?;
        let mut routes = Vec::with_capacity(config.routes.len());
        let mut draft = Router::draft();

        for ( index, spec ) in config.routes.iter().enumerate() {

            let rewrites = spec.rewrite.iter().map(|rule| Rewrite::compile(&rule.from, &rule.to, rule.status).map_err(|error| AppError::config("add_route", format!("route `{}`: {error}", spec.name)))).collect::<AppResult<Vec<_>>>()?;
            let files = spec.root.as_deref().map(|root| Files::new(root, spec.index.as_deref(), spec.cache_control.as_deref(), spec.autoindex).map_err(|error| AppError::config("add_route", format!("route `{}`: {error}", spec.name)))).transpose()?;

            let mut plan = Self::plan(config, spec, &catalog)?;
            let scripted = config.pools.get(&spec.upstream).is_some_and(|pool| pool.backends.iter().any(|backend| backend.protocol == Protocol::Fastcgi));
            let script = spec.root.as_ref().filter(|_| scripted).map(|root| Script { root: root.to_string_lossy().trim_end_matches('/').into(), index: "index.php".into(), suffix: ".php".into() });

            if !spec.replace.is_empty() { plan.request_headers.push(( http::header::ACCEPT_ENCODING, Rendered::Remove )); }
            let tagged = config.identity.propagate || spec.capture || spec.forward_auth.is_some() || config.identity.backend_block_header.is_some() || plan.dynamic_request() || plan.dynamic_response();

            let route = Arc::new(RouteState {
                index,
                name     : Arc::from(spec.name.as_str()),
                pool     : config.pools.keys().position(|name| *name == spec.upstream),
                mirror   : spec.mirror.as_ref().and_then(|mirror| config.pools.keys().position(|name| name == mirror)),
                spec     : spec.clone(),
                plan,
                policy   : Policy { tagged, ..Self::policy(config, spec) },
                checks   : Self::checks(spec, &catalog)?,
                replace  : (!spec.replace.is_empty()).then(|| Swaps { rules: spec.replace.iter().map(|( from, to )| ( Bytes::copy_from_slice(from.as_bytes()), Bytes::copy_from_slice(to.as_bytes()) )).collect(), types: spec.replace_types.iter().map(|kind| kind.to_ascii_lowercase().into_boxed_str()).collect() }),
                keyed    : spec.rate_key.as_deref().or(config.limits.rate_key.as_deref()).map(|key| HashKey::compile(Some(key), HashKey::Ip).map_err(|_| AppError::config("add_route", format!("route `{}`: unsupported rate_key `{key}`", spec.name)))).transpose()?,
                rules    : Self::rules(config, spec, index)?,
                bypass   : spec.cache_bypass.iter().filter_map(|name| catalog.index(name)).collect(),
                method   : spec.method.as_deref().map(|method| Method::from_bytes(method.as_bytes()).map_err(|_| AppError::config("add_route", format!("route `{}`: method `{method}` is not usable", spec.name)))).transpose()?,
                rewrites,
                files,
                tries    : match script.is_some() && spec.try_files.is_empty() { true => vec![Candidate::parse("$uri")], false => spec.try_files.iter().map(|entry| Candidate::parse(entry)).collect() },
                script,
                bearer   : spec.jwt.as_deref().and_then(|name| bearers.get(name).cloned()),
                link     : spec.secure_link.as_ref().map(Link::compile).transpose().map_err(|error| AppError::config("add_route", format!("route `{}`: {error}", spec.name)))?,
                auth     : spec.basic_auth.as_ref().map(Basic::compile).transpose().map_err(|error| AppError::config("add_route", format!("route `{}`: {error}", spec.name)))?,
                verify   : spec.forward_auth.as_ref().map(|auth| Self::verifier(config, auth)).transpose().map_err(|error| AppError::config("add_route", format!("route `{}`: {error}", spec.name)))?,
                errors   : (!spec.error_pages.is_empty()).then(|| Errors::compile(&spec.error_pages)).transpose()?,
                fence    : Fence::compile(&config.acl, &spec.acl),
                reply    : spec.respond.as_ref().map(Reply::compile).transpose()?,
            });

            match spec.regex.as_deref() {
                Some(regex) => draft.rule(spec.host.as_deref(), regex, route.clone()).map_err(|error| AppError::config("add_route", format!("route `{}`: {error}", spec.name)))?,
                None => { for pattern in Pattern::expand(&spec.path, spec.exact) { draft.add(spec.host.as_deref(), pattern, spec.exact, spec.prefer, route.clone()); } }
            }

            routes.push(route);

        }

        let router = draft.build(|left, right| Self::priority(&left.spec).cmp(&Self::priority(&right.spec)).then_with(|| left.name.cmp(&right.name)))?;

        Ok(( routes, router, catalog ))

    }

    pub fn accepts ( route: &RouteState, method: &Method, hint: Hint<'_>, catalog: &Catalog ) -> bool {

        if !route.spec.methods.is_empty() && !route.spec.methods.iter().any(|allowed| allowed == method.as_str()) { return false; }

        if route.spec.scheme.as_ref().is_some_and(|scheme| (scheme == "https") != hint.secure) { return false; }

        route.checks.iter().all(|check| check.passes(hint, catalog))

    }

    fn checks ( route: &Route, catalog: &Catalog ) -> AppResult<Vec<Check>> {

        let fail = |what: &str, name: &str| AppError::config("add_route", format!("route `{}`: {what} `{name}` is not usable", route.name));
        let mut checks = Vec::with_capacity(route.match_headers.len() + route.match_query.len() + route.match_vars.len());

        for ( name, expected ) in &route.match_headers { checks.push(Check::compile(Subject::Header(HeaderName::from_bytes(name.as_bytes()).map_err(|_| fail("header", name))?), expected)?); }

        for ( name, expected ) in &route.match_query { checks.push(Check::compile(Subject::Query(name.as_str().into()), expected)?); }

        for ( name, expected ) in &route.match_vars { checks.push(Check::compile(Subject::Derived(catalog.index(name).ok_or_else(|| fail("variable", name))?), expected)?); }

        Ok(checks)

    }

    fn rules ( config: &Config, route: &Route, index: usize ) -> AppResult<Vec<Rule>> {

        let ( source, base ) = if route.rate_rules.is_empty() { ( &config.limits.rate_rules, 0x8000_0000u32 ) } else { ( &route.rate_rules, 0x4000_0000 | ((index as u32 & 0x00ff_ffff) << 6) ) };

        source.iter().enumerate().map(|( position, rule )| Ok(Rule {
            key   : HashKey::compile(Some(rule.key.as_str()).filter(|key| !key.is_empty()), HashKey::Ip).map_err(|_| AppError::config("add_route", format!("route `{}`: unsupported rate_rules key `{}`", route.name, rule.key)))?,
            scope : base | position as u32,
            rate  : rule.rate,
            burst : rule.burst,
        })).collect()

    }

    fn policy ( config: &Config, route: &Route ) -> Policy {

        Policy {
            capture    : route.capture,
            rate_limit : route.rate_limit_10s.unwrap_or(config.limits.rate_limit_10s),
            scoped     : route.rate_limit_10s.is_some(),
            pace       : route.rate_per_second.unwrap_or(config.limits.rate_per_second),
            burst      : route.rate_burst.unwrap_or(config.limits.rate_burst),
            paced      : route.rate_per_second.is_some(),
            decisions  : config.decisions.enabled && route.decisions.unwrap_or(true),
            concurrency: route.concurrency_limit.unwrap_or(config.limits.concurrency_limit),
            tagged     : true,
        }

    }

    fn priority ( route: &Route ) -> ( u8, u8, usize ) {

        ( u8::from(!route.exact), u8::from(route.methods.is_empty()), usize::MAX - (route.match_headers.len() + route.match_query.len() + route.match_vars.len()) )

    }

    fn plan ( config: &Config, route: &Route, catalog: &Catalog ) -> AppResult<Plan> {

        let mut request_headers = Self::headers(&config.request_headers, catalog)?;
        let mut response_headers = Self::headers(&config.response_headers, catalog)?;

        for ( name, value ) in Self::headers(&route.request_headers, catalog)? {

            request_headers.retain(|( existing, _ )| *existing != name);
            request_headers.push(( name, value ));

        }

        for ( name, value ) in Self::headers(&route.response_headers, catalog)? {

            response_headers.retain(|( existing, _ )| *existing != name);
            response_headers.push(( name, value ));

        }

        Ok(Plan {
            timeout_ms       : route.timeout_ms.unwrap_or(config.limits.timeout_ms),
            max_body_bytes   : route.max_body_bytes.unwrap_or(config.limits.max_body_bytes),
            preserve_host    : route.preserve_host.unwrap_or(config.preserve_host),
            underscores      : config.server.underscores_in_headers,
            mount            : if route.strip_prefix { route.path.trim_end_matches('/').into() } else { Box::default() },
            client_timeout_ms: config.limits.client_timeout_ms,
            buffer_request   : route.buffer_request.unwrap_or(config.limits.buffer_requests),
            compress         : route.compress.unwrap_or(config.compression.enabled),
            cache            : route.cache.unwrap_or(config.cache.enabled),
            bandwidth        : route.bandwidth.unwrap_or(config.limits.bandwidth),
            bandwidth_after  : route.bandwidth_after.unwrap_or(config.limits.bandwidth_after),
            gunzip           : route.gunzip.unwrap_or(config.compression.gunzip),
            buffer_response  : if route.buffer_response.unwrap_or(config.limits.buffer_responses) { config.limits.response_buffer_bytes } else { 0 },
            request_headers,
            response_headers,
            redirects        : route.redirects.as_ref().map(|rules| rules.iter().map(|rule| Edit::new(&rule.from, &rule.to)).collect()),
            cookie_domain    : route.cookie_domain.iter().map(|rule| Edit::new(&rule.from, &rule.to)).collect(),
            cookie_path      : route.cookie_path.iter().map(|rule| Edit::new(&rule.from, &rule.to)).collect(),
        })

    }

    fn verifier ( config: &Config, auth: &ForwardAuth ) -> AppResult<Verifier> {

        let pool = config.pools.keys().position(|name| *name == auth.upstream).ok_or_else(|| AppError::config("forward_auth", format!("unknown pool `{}`", auth.upstream)))?;
        let copy = auth.copy_headers.iter().map(|name| Header::static_name(name).ok_or_else(|| AppError::config("forward_auth", format!("invalid header name `{name}`")))).collect::<AppResult<Vec<_>>>()?;
        let plan = Plan { timeout_ms: auth.timeout_ms, preserve_host: auth.preserve_host, client_timeout_ms: config.limits.client_timeout_ms, ..Plan::default() };

        Ok(Verifier { pool, path: auth.path.as_str().into(), copy, plan })

    }

    fn headers ( headers: &BTreeMap<String, String>, catalog: &Catalog ) -> AppResult<Vec<( HeaderName, Rendered )>> {

        headers.iter().map(|( key, value )| {

            let ( mode, bare ) = match key.as_bytes().first() { Some(mode @ (b'+' | b'-' | b'?')) => ( *mode, &key[1..] ), _ => ( b'=', key.as_str() ) };
            let name = Header::static_name(bare).ok_or_else(|| AppError::config("headers", format!("invalid header name `{bare}`")))?;

            if mode == b'-' { return Ok(( name, Rendered::Remove )); }

            let template = Template::compile_with(value, &|name| catalog.index(name))?;

            if !template.is_static() {

                if mode != b'=' { return Err(AppError::config("headers", format!("header `{key}` cannot combine a variable with `+` or `?`"))); }

                return Ok(( name, Rendered::Dynamic(template) ));

            }

            let value = Header::static_value(value).ok_or_else(|| AppError::config("headers", format!("invalid header value for `{name}`")))?;

            Ok(( name, match mode { b'+' => Rendered::Append(value), b'?' => Rendered::Default(value), _ => Rendered::Static(value) } ))

        }).collect()

    }

}
