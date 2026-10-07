use std::cmp::Ordering;
use std::collections::{BTreeMap, HashMap};

use foldhash::fast::RandomState;

use crate::core::error::{AppError, AppResult};
use crate::core::matcher::Matcher;
use crate::http::request::Request;
use super::arch::{Draft, Entry, Pattern, Router, Scope};

impl <T> Router <T> {

    pub fn draft () -> Draft<T> {

        Draft { exact: HashMap::new(), wildcard: BTreeMap::new(), any: BTreeMap::new(), rules: Vec::new() }

    }

    pub fn find <'a> ( &'a self, host: &str, path: &str, accept: impl Fn(&T) -> bool ) -> Option<&'a T> {

        if let Some(single) = &self.single { return accept(single).then_some(single); }

        let found = self.lookup(host, path, &accept);

        if self.rules.is_empty() || found.is_some_and(|entry| entry.exact || entry.prefer) { return found.map(|entry| &entry.value); }

        let name = Request::host_name(host).trim_end_matches('.');

        for ( scope, regex, value ) in &self.rules {

            if scope.covers(name) && regex.is_match(path) && accept(value) { return Some(value); }

        }

        found.map(|entry| &entry.value)

    }

    fn lookup <'a> ( &'a self, host: &str, path: &str, accept: &impl Fn(&T) -> bool ) -> Option<&'a Entry<T>> {

        let lowered;
        let host = Request::host_name(host);
        let host = if host.bytes().any(|byte| byte.is_ascii_uppercase()) { lowered = host.to_ascii_lowercase(); lowered.as_str() } else { host };
        let host = host.trim_end_matches('.');

        if let Some(matcher) = self.exact.get(host) && let Some(found) = Self::pick(matcher, path, accept) { return Some(found); }

        for ( suffix, matcher ) in &self.wildcard {

            if host.len() > suffix.len() && host.ends_with(suffix.as_str()) && let Some(found) = Self::pick(matcher, path, accept) { return Some(found); }

        }

        Self::pick(&self.any, path, accept)

    }

    pub fn describe ( &self ) -> Vec<String> {

        let mut lines: Vec<String> = self.exact.keys().map(|host| format!("host {host}")).collect();

        lines.extend(self.wildcard.iter().map(|( suffix, _ )| format!("wildcard *{suffix}")));
        lines.extend(self.rules.iter().map(|( _, regex, _ )| format!("regex {}", regex.as_str())));

        lines

    }

    fn pick <'a> ( matcher: &'a Matcher<Vec<Entry<T>>>, path: &str, accept: &impl Fn(&T) -> bool ) -> Option<&'a Entry<T>> {

        matcher.find(path)?.iter().find(|entry| accept(&entry.value))

    }

}

impl <T: Clone> Draft <T> {

    pub fn add ( &mut self, host: Option<&str>, pattern: String, exact: bool, prefer: bool, value: T ) {

        let bucket = match host {
            Some(host) if host.starts_with("*.") => self.wildcard.entry(host[1..].to_string()).or_default(),
            Some(host) => self.exact.entry(host.to_string()).or_default(),
            None => &mut self.any,
        };

        bucket.entry(pattern).or_default().push(Entry { value, exact, prefer });

    }

    pub fn rule ( &mut self, host: Option<&str>, pattern: &str, value: T ) -> AppResult<()> {

        let regex = regex::Regex::new(pattern).map_err(|error| AppError::invalid("route regex", format!("{pattern}: {error}")))?;

        self.rules.push(( Scope::of(host), regex, value ));

        Ok(())

    }

    pub fn build ( self, order: impl Fn(&T, &T) -> Ordering ) -> AppResult<Router<T>> {

        let compile = |patterns: BTreeMap<String, Vec<Entry<T>>>| -> AppResult<Matcher<Vec<Entry<T>>>> {

            let mut matcher = Matcher::new();

            for ( pattern, mut candidates ) in patterns {

                candidates.sort_by(|left, right| order(&left.value, &right.value));
                matcher.insert(&pattern, candidates)?;

            }

            Ok(matcher)

        };

        let single = (self.rules.is_empty() && self.exact.is_empty() && self.wildcard.is_empty() && self.any.len() == 2 && self.any.values().all(|candidates| candidates.len() == 1) && self.any.contains_key("/{*rest}"))
            .then(|| self.any.get("/").and_then(|candidates| candidates.first().map(|entry| entry.value.clone())))
            .flatten();

        let mut exact = HashMap::with_capacity_and_hasher(self.exact.len(), RandomState::default());

        for ( host, patterns ) in self.exact { exact.insert(host, compile(patterns)?); }

        let mut wildcard = Vec::with_capacity(self.wildcard.len());

        for ( suffix, patterns ) in self.wildcard { wildcard.push(( suffix, compile(patterns)? )); }

        wildcard.sort_by(|left, right| right.0.len().cmp(&left.0.len()).then_with(|| left.0.cmp(&right.0)));

        Ok(Router { exact, wildcard, any: compile(self.any)?, single, rules: self.rules })

    }

}

impl Scope {

    pub fn of ( host: Option<&str> ) -> Self {

        match host {
            Some(host) if host.starts_with("*.") => Self::Suffix(host[1..].to_ascii_lowercase()),
            Some(host) => Self::Host(host.to_ascii_lowercase()),
            None => Self::Any,
        }

    }

    pub fn covers ( &self, host: &str ) -> bool {

        match self {
            Self::Any => true,
            Self::Host(name) => name.eq_ignore_ascii_case(host),
            Self::Suffix(suffix) => host.len() > suffix.len() && host.as_bytes()[host.len() - suffix.len()..].eq_ignore_ascii_case(suffix.as_bytes()),
        }

    }

}

impl Pattern {

    pub fn expand ( path: &str, exact: bool ) -> Vec<String> {

        let trimmed = path.trim_end_matches('/');
        let base = if trimmed.is_empty() { "/".to_string() } else { trimmed.to_string() };

        if exact { return vec![base]; }

        if base == "/" { return vec![base, "/{*rest}".to_string()]; }

        vec![format!("{base}/"), format!("{base}/{{*rest}}"), base]

    }

}
