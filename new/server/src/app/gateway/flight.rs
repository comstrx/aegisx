use std::sync::Arc;

use crate::app::{Actor, History, Identity, Key, Trace};
use crate::http::body::Body;
use crate::http::proxy::{Forward, Target};
use crate::http::request::Req;
use crate::http::upstream::Upstream;
use super::arch::{Flight, Rare};

impl <'a> Flight <'a> {

    pub(super) fn rare ( &mut self ) -> &mut Rare {

        self.rare.get_or_insert_with(Box::default)

    }

    pub(super) fn trace ( &mut self ) -> Option<&mut Trace> {

        self.rare.as_deref_mut().and_then(|rare| rare.trace.as_mut())

    }

    pub(super) fn traced ( &mut self ) -> Option<Trace> {

        self.rare.as_deref_mut().and_then(|rare| rare.trace.take())

    }

    pub(super) fn watched ( &self ) -> Option<&( Key, Arc<Actor>, History )> {

        self.rare.as_deref().and_then(|rare| rare.watched.as_ref())

    }

    pub(super) fn actor ( &self ) -> Option<&Arc<Actor>> {

        self.watched().map(|( _, actor, _ )| actor)

    }

    pub(super) fn path <'r> ( &'r self, request: &'r Req<Body> ) -> &'r str {

        self.rare.as_deref().and_then(|rare| rare.canonical.as_deref()).unwrap_or_else(|| request.uri().path())

    }

    pub(super) fn forward <'f> ( &'f self, upstream: &'f Upstream ) -> Forward<'f> {

        let snapshot = &self.runtime.snapshot;
        let rare = self.rare.as_deref();

        let target = match rare {
            Some(Rare { rewritten: Some(target), .. }) => Target::Rewritten(target),
            Some(Rare { canonical: Some(path), .. }) => Target::Canonical(path),
            _ => Target::Request,
        };

        Forward {
            plan     : &self.route.plan,
            upstream,
            target,
            add      : Identity::outgoing(snapshot, &self.context.peer, rare.and_then(|rare| rare.chain.as_ref()), &self.id),
            rendered : rare.map(|rare| rare.rendered.as_slice()).unwrap_or_default(),
            drop     : Identity::drops(snapshot, &self.context.peer),
            upgrade  : rare.and_then(|rare| rare.upgrade.as_ref()),
        }

    }

}
