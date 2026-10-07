use std::sync::Arc;

use http::Method;
use http::header::HeaderMap;

use crate::app::{Errors, Hint, Names, RouteState, Routes};
use crate::config::{AnalysisMode, Config};
use crate::core::error::AppResult;
use crate::core::net::Nets;
use super::arch::Snapshot;

impl Snapshot {

    pub fn build ( config: Config, version: u64 ) -> AppResult<Self> {

        let quota = config.limits.max_in_flight.div_ceil(config.worker_count().max(1)).max(1);
        let actors = config.analysis.mode != AnalysisMode::Off || config.decisions.enabled || config.limits.rate_limit_10s > 0 || config.limits.rate_per_second > 0 || config.limits.concurrency_limit > 0 || !config.limits.rate_rules.is_empty() || config.routes.iter().any(|route| !route.rate_rules.is_empty() || route.rate_limit_10s.is_some_and(|limit| limit > 0) || route.rate_per_second.is_some_and(|rate| rate > 0) || route.concurrency_limit.is_some_and(|limit| limit > 0));
        let names = Names::compile(&config)?;
        let ( routes, router, catalog ) = Routes::compile(&config)?;

        let compression = config.compression();
        let alt_svc = config.alt_svc();
        let errors = Errors::compile(&config.error_pages)?;
        let pages = !errors.is_empty() || routes.iter().any(|route| route.errors.is_some());
        let paced = routes.iter().any(|route| route.plan.bandwidth > 0);
        let internal = routes.iter().any(|route| route.spec.internal);
        let trusted = Nets::new(&config.identity.trusted_peers);

        Ok(Self { version, quota, actors, config: Arc::new(config), routes, router, catalog, names, compression, alt_svc, errors, pages, internal, paced, trusted })

    }

    pub fn route ( &self, host: &str, path: &str, method: &Method, headers: &HeaderMap ) -> Option<&Arc<RouteState>> {

        self.locate(host, path, method, Hint { ip: std::net::IpAddr::V4(std::net::Ipv4Addr::UNSPECIFIED), secure: false, headers, uri: &http::Uri::from_static("/") })

    }

    pub fn locate ( &self, host: &str, path: &str, method: &Method, hint: Hint<'_> ) -> Option<&Arc<RouteState>> {

        self.router.find(host, path, |route| Routes::accepts(route, method, hint, &self.catalog))

    }

    pub fn describe ( &self ) -> Vec<String> {

        let mut lines = self.router.describe();

        lines.extend(self.routes.iter().map(|route| format!("route {} host={:?} path={} exact={}", route.name, route.spec.host, route.spec.path, route.spec.exact)));

        lines

    }

}
