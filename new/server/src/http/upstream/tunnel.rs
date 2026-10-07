use std::any::Any;
use std::rc::Rc;
use std::time::Duration;

use hyper_util::rt::TokioIo;
use tokio::io::{AsyncRead, AsyncWrite};
use tokio_io_timeout::TimeoutStream;

use crate::core::log::debug;
use crate::core::rt::Rt;
use crate::http::io::Upgrading;
use super::arch::{Streaming, Tunnel};

impl Tunnel {

    pub fn bridge ( upgrading: Upgrading, upstream: Streaming, idle_ms: u64, keep: Option<Rc<dyn Any>> ) {

        let Some(pending) = upstream.detach() else { debug!("upstream upgrade has nothing to hand over"); return; };

        Rt::spawn_local(async move {

            let _held = keep;

            let upstream = match pending.await {
                Ok(upgraded) => TokioIo::new(upgraded),
                Err(error) => { debug!(%error, "upstream side of the upgrade did not complete"); return; }
            };

            match upgrading.wait().await {
                Ok(client) => { if let Err(error) = Self::join(client, upstream, idle_ms).await { debug!(%error, "tunnel ended"); } }
                Err(error) => debug!(%error, "client side of the upgrade did not complete"),
            }

        });

    }

    pub async fn join <A, B> ( left: A, right: B, idle_ms: u64 ) -> std::io::Result<( u64, u64 )>
    where A: AsyncRead + AsyncWrite, B: AsyncRead + AsyncWrite {

        let idle = Some(Duration::from_millis(idle_ms.max(1)));
        let ( mut left, mut right ) = ( TimeoutStream::new(left), TimeoutStream::new(right) );

        left.set_read_timeout(idle);
        right.set_read_timeout(idle);

        let ( mut left, mut right ) = ( Box::pin(left), Box::pin(right) );

        tokio::io::copy_bidirectional(&mut left, &mut right).await

    }

}
