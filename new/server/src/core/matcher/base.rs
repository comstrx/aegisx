use crate::core::error::{AppError, AppResult};
use super::arch::Matcher;

impl <T> Matcher <T> {

    pub fn new () -> Self {

        Self { inner: matchit::Router::new() }

    }

    pub fn insert ( &mut self, pattern: &str, value: T ) -> AppResult<()> {

        self.inner.insert(pattern, value).map_err(|error| AppError::invalid("route pattern", format!("{pattern}: {error}")))

    }

    pub fn find ( &self, path: &str ) -> Option<&T> {

        self.inner.at(path).ok().map(|found| found.value)

    }

    pub fn find_mut ( &mut self, path: &str ) -> Option<&mut T> {

        self.inner.at_mut(path).ok().map(|found| found.value)

    }

}

impl <T> Default for Matcher <T> {

    fn default () -> Self {

        Self::new()

    }

}
