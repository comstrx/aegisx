use std::borrow::Cow;

use http::StatusCode;
use regex::Regex;

use crate::core::error::{AppError, AppResult};
use super::arch::{Outcome, Rewrite};

impl Rewrite {

    pub fn compile ( from: &str, to: &str, status: Option<u16> ) -> AppResult<Self> {

        let regex = Regex::new(from).map_err(|error| AppError::config("rewrite", format!("invalid pattern `{from}`: {error}")))?;
        let absolute = to.starts_with("http://") || to.starts_with("https://");

        let redirect = match status {
            Some(code) => Some(StatusCode::from_u16(code).ok().filter(StatusCode::is_redirection).ok_or_else(|| AppError::config("rewrite", format!("status {code} is not a redirect status")))?),
            None if absolute => Some(StatusCode::FOUND),
            None => None,
        };

        let ( to, query ) = match to.strip_suffix('?') { Some(bare) => ( bare.to_string(), false ), None => ( to.to_string(), true ) };

        if to.is_empty() { return Err(AppError::config("rewrite", "replacement must not be empty")); }

        if redirect.is_none() && !to.starts_with('/') && !to.starts_with('$') { return Err(AppError::config("rewrite", format!("replacement `{to}` must start with /"))); }

        Ok(Self { regex, to, redirect, query })

    }

    pub fn run ( rules: &[Self], path: &str, query: Option<&str> ) -> Outcome {

        let mut current: Option<String> = None;
        let mut query: Option<Cow<'_, str>> = query.filter(|query| !query.is_empty()).map(Cow::Borrowed);

        for rule in rules {

            let subject = current.as_deref().unwrap_or(path);
            let Some(captures) = rule.regex.captures(subject) else { continue; };
            let mut target = String::with_capacity(rule.to.len() + subject.len());

            captures.expand(&rule.to, &mut target);

            if !rule.query { query = None; }

            if let Some(status) = rule.redirect { return Outcome::Redirect(Self::join(target, query.as_deref()), status); }

            if let Some(at) = target.find('?') {

                let own = target.split_off(at + 1);

                target.pop();
                query = Some(Cow::Owned(Self::merge(own, query.as_deref())));

            }

            current = Some(target);

        }

        match current {
            Some(target) => Outcome::Target(Self::join(target, query.as_deref())),
            None => Outcome::Keep,
        }

    }

    fn join ( mut target: String, query: Option<&str> ) -> String {

        if let Some(query) = query { target.push(if target.contains('?') { '&' } else { '?' }); target.push_str(query); }

        target

    }

    fn merge ( own: String, query: Option<&str> ) -> String {

        match query { Some(query) if !own.is_empty() => format!("{own}&{query}"), Some(query) => query.to_string(), None => own }

    }

}
