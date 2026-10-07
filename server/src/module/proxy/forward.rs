use pingora::http::RequestHeader;
use pingora::{Error, ErrorType};
use super::{Context, Proxy};

impl Proxy {
    pub(super) fn prepare ( &self, request: &mut RequestHeader, context: &Context ) -> pingora::Result<()> {

        let route = context.route.as_ref().expect("admitted route");
        let lease = context.lease.as_ref().expect("selected upstream");
        let untrusted: Vec<_> = request.headers.keys().filter(|name| {
            let name = name.as_str();
            name == "forwarded" || name == "x-real-ip" || name.starts_with("x-forwarded-")
        }).cloned().collect();
        for name in untrusted { request.remove_header(&name); }
        for (name, value) in &route.policy.request_headers { request.insert_header(name.clone(), value.clone())?; }
        let identity = &context.snapshot.config.identity;
        let names = &context.snapshot.identity_headers;
        request.remove_header("x-request-id");
        if names.request_id != "x-request-id" { request.remove_header(&names.request_id); }
        request.remove_header(&names.forwarded_for);
        request.remove_header(&names.forwarded_proto);
        if let Some(name) = &identity.backend_block_header { request.remove_header(name); }
        if let Some(value)=&context.id_value { request.insert_header(names.request_id.clone(), value.clone())?; }
        if !route.policy.preserve_host { request.insert_header("host", lease.backend.authority.clone())?; }
        if let Some(peer) = context.peer {
            if let Some(name) = &identity.actor_header && !crate::module::identity::Identity::trusted(identity, peer) { request.remove_header(name); }
            if identity.forwarding {
                request.insert_header(names.forwarded_for.clone(), peer.to_string())?;
                request.insert_header(names.forwarded_proto.clone(), if context.snapshot.config.tls.is_some() { "https" } else { "http" })?;
            }
        }
        let path = context.canonical.as_deref().unwrap_or(request.uri.path());
        if route.spec.strip_prefix || path != request.uri.path() {
            let path = if route.spec.strip_prefix { path.strip_prefix(&route.spec.path).unwrap_or(path) } else { path };
            let path = if path.is_empty() { "/" .to_owned() } else if path.starts_with('/') { path.to_owned() } else { format!("/{path}") };
            let uri = match request.uri.query() { Some(query) => format!("{path}?{query}"), None => path };
            request.set_uri(uri.parse().map_err(|_| Error::explain(ErrorType::HTTPStatus(400), "Invalid rewritten URI"))?);
        }

        Ok(())

    }

}
