use bytes::Bytes;
use http::StatusCode;
use http::header::{CONTENT_LENGTH, CONTENT_TYPE, HeaderValue};

use crate::http::body::Body;
use super::arch::{Res, Response};

impl Response {

    pub fn status ( code: u16 ) -> Res {

        let mut response = Res::new(Body::Empty);

        *response.status_mut() = StatusCode::from_u16(code).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
        response.headers_mut().insert(CONTENT_LENGTH, HeaderValue::from_static("0"));

        response

    }

    pub fn text ( code: u16, text: &'static str ) -> Res {

        let mut response = Self::status(code);

        response.headers_mut().insert(CONTENT_LENGTH, HeaderValue::from(text.len()));
        response.headers_mut().insert(CONTENT_TYPE, HeaderValue::from_static("text/plain; charset=utf-8"));
        *response.body_mut() = Body::bytes(text);

        response

    }

    pub fn bytes ( code: u16, content_type: &'static str, body: impl Into<Bytes> ) -> Res {

        let body = body.into();
        let mut response = Self::status(code);

        response.headers_mut().insert(CONTENT_LENGTH, HeaderValue::from(body.len()));
        response.headers_mut().insert(CONTENT_TYPE, HeaderValue::from_static(content_type));
        *response.body_mut() = Body::bytes(body);

        response

    }

}
