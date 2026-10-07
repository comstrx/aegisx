use crate::app::{Hint, Identity, Lease};
use crate::core::log::debug;
use crate::core::rt::Rt;
use crate::http::body::Body;
use crate::http::proxy::{Forward, Proxy, Target};
use crate::http::request::{Req, Request};
use crate::http::upstream::Replay;
use super::arch::{Flight, Handler};

impl Handler {

    pub(super) fn mirror ( &self, flight: &Flight<'_>, request: &Req<Body>, pool: usize ) {

        let runtime = self.runtime.get();
        let Some(pool) = runtime.pools.get(pool).cloned() else { return; };
        let Some(route) = runtime.snapshot.routes.get(flight.route.index).cloned() else { return; };
        let Some(copy) = self.arena.with_mut(|arena| Request::shadow(request, arena)).and_then(|shadow| shadow.restore()) else { return; };
        let now = runtime.pools.since(flight.started);
        let hint = Hint { ip: flight.context.peer.addr.ip(), secure: flight.context.peer.proto.as_bytes() == b"https", headers: request.headers(), uri: request.uri() };
        let Some(backend) = self.picker.with_mut(|picker| picker.pick(&runtime.pools, &pool, &[], now, hint)).and_then(|index| pool.backends.get(index)).cloned() else { return; };
        let rare = flight.rare.as_deref();
        let canonical = rare.and_then(|rare| rare.canonical.clone());
        let rewritten = rare.and_then(|rare| rare.rewritten.clone());
        let chain = rare.and_then(|rare| rare.chain.clone());
        let rendered = rare.map(|rare| rare.rendered.clone()).unwrap_or_default();
        let ( client, context, id, started ) = ( self.client.clone(), flight.context.clone(), flight.id.clone(), flight.started );

        Rt::spawn_local(async move {

            let snapshot = &runtime.snapshot;

            let target = match ( &rewritten, &canonical ) {
                ( Some(target), _ ) => Target::Rewritten(target),
                ( None, Some(path) ) => Target::Canonical(path),
                ( None, None ) => Target::Request,
            };

            let forward = Forward { plan: &route.plan, upstream: &backend.upstream, target, add: Identity::outgoing(snapshot, &context.peer, chain.as_ref(), &id), rendered: &rendered, drop: Identity::drops(snapshot, &context.peer), upgrade: None };
            let lease = pool.counting.then(|| Lease::new(backend.clone()));

            match Proxy::forward(&client, &forward, started, Replay::default(), copy).await {
                Ok(_) => backend.succeed(),
                Err(failure) => {

                    if failure.connect { backend.fail(&pool, now); }

                    debug!(backend = %backend.addr, error = %failure.error, "mirrored request failed");

                }
            }

            drop(lease);

        });

    }

}
