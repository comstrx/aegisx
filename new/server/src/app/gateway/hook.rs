use std::rc::Rc;

use http::header::HOST;

use crate::app::{Hooks, Seen, Snapshot, View};
use crate::core::log::warn;
use crate::http::body::Body;
use crate::http::request::Req;
use crate::http::response::Res;
use super::arch::{Flight, Handler};

impl Handler {

    fn vm ( &self, snapshot: &Snapshot ) -> Option<Rc<Hooks>> {

        self.hooks.with_mut(|slot| {

            if slot.0 != snapshot.version {

                *slot = ( snapshot.version, match Hooks::load(&snapshot.config.hooks) {
                    Ok(hooks) => Some(Rc::new(hooks)),
                    Err(error) => { warn!(%error, "lua hooks did not load"); None }
                } );

            }

            slot.1.clone()

        })

    }

    pub(super) fn hooked ( &self, snapshot: &Snapshot, flight: &mut Flight<'_>, request: &mut Req<Body> ) -> Result<Option<Res<Body>>, u16> {

        let hooks = self.vm(snapshot).ok_or(500u16)?;
        let host = request.headers().get(HOST).cloned();
        let ( method, uri ) = ( request.method().clone(), request.uri().clone() );

        if snapshot.config.hooks.response { flight.rare().seen = Some(Box::new(Seen { method: method.clone(), uri: uri.clone(), host: host.clone() })); }

        let view = View { method: &method, uri: &uri, host: host.as_ref(), ip: flight.context.peer.addr.ip(), secure: flight.context.peer.proto.as_bytes() == b"https" };

        hooks.request(view, request.headers_mut()).map_err(|error| { warn!(%error, "on_request failed"); 500 })

    }

    pub(super) fn answered ( &self, flight: &Flight<'_>, response: &mut Res<Body> ) {

        let Some(seen) = flight.rare.as_deref().and_then(|rare| rare.seen.as_deref()) else { return; };
        let Some(hooks) = self.vm(&flight.runtime.snapshot) else { return; };
        let view = View { method: &seen.method, uri: &seen.uri, host: seen.host.as_ref(), ip: flight.context.peer.addr.ip(), secure: flight.context.peer.proto.as_bytes() == b"https" };

        if let Err(error) = hooks.response(view, response) { warn!(%error, "on_response failed"); }

    }

}
