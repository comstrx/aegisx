use std::borrow::Cow;
use std::ops::Range;

use bytes::BufMut;
use http::header::{HeaderName, HeaderValue};
use http::{Method, Uri, Version};

use crate::core::arena::Arena;
use crate::http::body::Body;
use super::arch::{Req, Request, Shadow};

impl Request {

    pub fn shadow ( request: &Req<Body>, arena: &mut Arena ) -> Option<Shadow> {

        let body = match request.body().replay()? { Body::Empty => None, body => Some(Box::new(body)) };
        let uri = request.uri();
        let method = request.method().as_str().as_bytes();
        let headers = request.headers();

        let target = match uri.scheme().is_none() && uri.authority().is_none() {
            true => Cow::Borrowed(Self::target(uri)),
            false => Cow::Owned(uri.to_string()),
        };

        let room = 9 + method.len() + target.len() + headers.iter().map(|( name, value )| 6 + name.as_str().len() + value.len()).sum::<usize>();

        let head = arena.write(room, |buffer| {

            buffer.put_u8(Self::code(request.version()));
            buffer.put_u16_le(method.len() as u16);
            buffer.put_slice(method);
            buffer.put_u32_le(target.len() as u32);
            buffer.put_slice(target.as_bytes());
            buffer.put_u16_le(headers.len() as u16);

            for ( name, value ) in headers {

                buffer.put_u16_le(name.as_str().len() as u16);
                buffer.put_u32_le(value.len() as u32);
                buffer.put_slice(name.as_str().as_bytes());
                buffer.put_slice(value.as_bytes());

            }

        });

        Some(Shadow { head, body })

    }

    fn code ( version: Version ) -> u8 {

        match version {
            Version::HTTP_09 => 0,
            Version::HTTP_10 => 1,
            Version::HTTP_2 => 3,
            Version::HTTP_3 => 4,
            _ => 2,
        }

    }

    fn version ( code: u8 ) -> Version {

        match code {
            0 => Version::HTTP_09,
            1 => Version::HTTP_10,
            3 => Version::HTTP_2,
            4 => Version::HTTP_3,
            _ => Version::HTTP_11,
        }

    }

}

impl Shadow {

    pub fn restore ( &self ) -> Option<Box<Req<Body>>> {

        let head = &self.head;
        let mut cursor = 0usize;

        let mut take = |count: usize| -> Option<Range<usize>> {

            let end = cursor.checked_add(count).filter(|end| *end <= head.len())?;
            let range = cursor..end;

            cursor = end;

            Some(range)

        };

        let version = Request::version(head[take(1)?][0]);
        let length = Self::number(&head[take(2)?])?;
        let method = take(length)?;
        let length = Self::number(&head[take(4)?])?;
        let target = take(length)?;
        let count = Self::number(&head[take(2)?])?;
        let mut request = Box::new(Req::new(match &self.body { Some(body) => body.replay()?, None => Body::Empty }));

        *request.method_mut() = Method::from_bytes(&head[method]).ok()?;
        *request.uri_mut() = Uri::from_maybe_shared(head.slice(target)).ok()?;
        *request.version_mut() = version;

        request.headers_mut().reserve(count);

        for _ in 0..count {

            let name = Self::number(&head[take(2)?])?;
            let value = Self::number(&head[take(4)?])?;
            let name = take(name)?;
            let value = take(value)?;
            let name = HeaderName::from_bytes(&head[name]).ok()?;
            let value = HeaderValue::from_maybe_shared(head.slice(value)).ok()?;

            request.headers_mut().append(name, value);

        }

        Some(request)

    }

    fn number ( bytes: &[u8] ) -> Option<usize> {

        match bytes.len() {
            2 => Some(usize::from(u16::from_le_bytes(bytes.try_into().ok()?))),
            4 => Some(u32::from_le_bytes(bytes.try_into().ok()?) as usize),
            _ => None,
        }

    }

}
