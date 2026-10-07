use std::net::IpAddr;

use bytes::Bytes;
use http::StatusCode;
use http::header::{CONTENT_LENGTH, CONTENT_TYPE};

use crate::config::{Acl, Respond};
use crate::core::error::{AppError, AppResult};
use crate::core::net::Nets;
use crate::core::str::Str;
use crate::http::body::Body;
use crate::http::header::Header;
use crate::http::response::Res;
use super::arch::{Fence, Reply};

impl Fence {

    pub fn compile ( global: &Acl, route: &Acl ) -> Option<Self> {

        let allow = if route.allow.is_empty() { &global.allow } else { &route.allow };
        let deny: Vec<_> = global.deny.iter().chain(&route.deny).copied().collect();

        (!allow.is_empty() || !deny.is_empty()).then(|| Self { allow: Nets::new(allow), deny: Nets::new(&deny) })

    }

    pub fn admits ( &self, ip: IpAddr ) -> bool {

        !self.deny.contains(ip) && (self.allow.is_empty() || self.allow.contains(ip))

    }

}

impl Reply {

    pub fn compile ( respond: &Respond ) -> AppResult<Self> {

        let status = StatusCode::from_u16(respond.status).map_err(|_| AppError::config("add_route", format!("respond status {} is invalid", respond.status)))?;
        let kind = Header::static_value(&respond.content_type).ok_or_else(|| AppError::config("add_route", "respond content_type is invalid"))?;
        let length = Header::static_value(&respond.body.len().to_string()).ok_or_else(|| AppError::config("add_route", "respond body length is invalid"))?;

        Ok(Self { status, kind, length, body: Bytes::from_static(Str::intern(&respond.body).as_bytes()) })

    }

    pub fn response ( &self ) -> Res<Body> {

        let mut response = Res::new(Body::bytes(self.body.clone()));

        *response.status_mut() = self.status;
        response.headers_mut().insert(CONTENT_TYPE, self.kind.clone());
        response.headers_mut().insert(CONTENT_LENGTH, self.length.clone());

        response

    }

}
