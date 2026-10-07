use http::header::HeaderValue;

use crate::config::ErrorPage;
use crate::core::error::{AppError, AppResult};
use super::arch::{Errors, Page};

impl Errors {

    pub fn compile ( pages: &[ErrorPage] ) -> AppResult<Self> {

        let pages = pages.iter().map(|page| {

            let redirect = (page.page.starts_with("http://") || page.page.starts_with("https://"))
                .then(|| HeaderValue::from_str(&page.page).map_err(|_| AppError::config("error_pages", format!("invalid redirect `{}`", page.page))))
                .transpose()?;

            Ok(Page { statuses: page.status.clone(), redirect, path: page.page.as_str().into(), code: page.code })

        }).collect::<AppResult<Vec<_>>>()?;

        Ok(Self { pages })

    }

    pub fn is_empty ( &self ) -> bool {

        self.pages.is_empty()

    }

    pub fn find ( &self, status: u16 ) -> Option<&Page> {

        self.pages.iter().find(|page| page.statuses.contains(&status))

    }

}
