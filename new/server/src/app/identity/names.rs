use crate::config::Config;
use crate::core::error::{AppError, AppResult};
use crate::http::header::{FORWARDED, Header, X_REAL_IP};
use super::arch::Names;

impl Names {

    pub fn compile ( config: &Config ) -> AppResult<Self> {

        let name = |key: &str, text: &str| Header::static_name(text).ok_or_else(|| AppError::config(key, format!("invalid header name `{text}`")));

        let request_id = name("identity.request_id_header", &config.identity.request_id_header)?;
        let forwarded_for = name("identity.forwarded_for_header", &config.identity.forwarded_for_header)?;
        let forwarded_proto = name("identity.forwarded_proto_header", &config.identity.forwarded_proto_header)?;
        let actor = config.identity.actor_header.as_deref().map(|text| name("identity.actor_header", text)).transpose()?;
        let block = config.identity.backend_block_header.as_deref().map(|text| name("identity.backend_block_header", text)).transpose()?;

        let mut drop_trusted = vec![FORWARDED, X_REAL_IP];

        drop_trusted.extend(block.clone());

        if !config.identity.forwarding { drop_trusted.push(forwarded_for.clone()); drop_trusted.push(forwarded_proto.clone()); }

        if !config.identity.propagate { drop_trusted.push(request_id.clone()); }

        let mut drop_untrusted = drop_trusted.clone();

        drop_untrusted.extend(actor.clone());

        if config.identity.forwarding { drop_untrusted.push(forwarded_for.clone()); drop_untrusted.push(forwarded_proto.clone()); }

        if config.identity.propagate { drop_untrusted.push(request_id.clone()); }

        Ok(Self { request_id, forwarded_for, forwarded_proto, actor, block, drop_trusted, drop_untrusted })

    }

}
