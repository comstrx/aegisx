use std::cell::RefCell;
use std::io::IoSlice;
use std::net::{SocketAddr, TcpListener};
use std::pin::Pin;
use std::task::{Context, Poll};

use socket2::{Domain, Protocol, Socket as RawSocket, Type};
use tokio::io::AsyncWrite;
use tokio::net::TcpStream;

use crate::core::error::{AppFail, AppResult};
use super::arch::{GATHER_BYTES, Socket};

thread_local! {
    static SCRATCH: RefCell<Vec<u8>> = RefCell::new(Vec::with_capacity(GATHER_BYTES));
}

impl Socket {

    pub fn listen ( addr: SocketAddr, backlog: i32, shared: bool ) -> AppResult<TcpListener> {

        let socket = RawSocket::new(Domain::for_address(addr), Type::STREAM, Some(Protocol::TCP)).or_fail("cannot create socket")?;

        socket.set_reuse_address(true).or_fail("cannot set reuse address")?;

        #[cfg(all(unix, not(any(target_os = "solaris", target_os = "illumos"))))]
        if shared { socket.set_reuse_port(true).or_fail("cannot set reuse port")?; }

        #[cfg(not(all(unix, not(any(target_os = "solaris", target_os = "illumos")))))]
        let _ = shared;

        socket.set_nonblocking(true).or_fail("cannot set nonblocking")?;
        socket.bind(&addr.into()).or_fail_with(|| format!("cannot bind {addr}"))?;
        socket.listen(backlog).or_fail_with(|| format!("cannot listen on {addr}"))?;

        Ok(socket.into())

    }

    pub fn datagram ( addr: SocketAddr, shared: bool ) -> AppResult<std::net::UdpSocket> {

        let socket = RawSocket::new(Domain::for_address(addr), Type::DGRAM, Some(Protocol::UDP)).or_fail("cannot create udp socket")?;

        socket.set_reuse_address(true).or_fail("cannot set reuse address")?;

        #[cfg(all(unix, not(any(target_os = "solaris", target_os = "illumos"))))]
        if shared { socket.set_reuse_port(true).or_fail("cannot set reuse port")?; }

        #[cfg(not(all(unix, not(any(target_os = "solaris", target_os = "illumos")))))]
        let _ = shared;

        socket.set_nonblocking(true).or_fail("cannot set nonblocking")?;
        socket.bind(&addr.into()).or_fail_with(|| format!("cannot bind udp {addr}"))?;

        Ok(socket.into())

    }

    pub fn tune ( stream: &TcpStream ) {

        let _ = stream.set_nodelay(true);

    }

    pub fn gather <W: AsyncWrite + ?Sized> ( writer: Pin<&mut W>, cx: &mut Context<'_>, bufs: &[IoSlice<'_>] ) -> Poll<std::io::Result<usize>> {

        match bufs {
            [] => Poll::Ready(Ok(0)),
            [single] => writer.poll_write(cx, single),
            many => {

                if many.iter().map(|buf| buf.len()).sum::<usize>() > GATHER_BYTES { return writer.poll_write_vectored(cx, many); }

                SCRATCH.with(|scratch| {

                    let mut scratch = scratch.borrow_mut();

                    scratch.clear();

                    for buf in many { scratch.extend_from_slice(buf); }

                    writer.poll_write(cx, &scratch)

                })

            }
        }

    }

    #[cfg(unix)]
    pub fn listen_unix ( path: &str, backlog: i32 ) -> AppResult<std::os::unix::net::UnixListener> {

        use std::os::unix::fs::FileTypeExt;
        use std::os::unix::net::UnixListener;

        if let Ok(meta) = std::fs::symlink_metadata(path) && meta.file_type().is_socket() { std::fs::remove_file(path).or_fail_with(|| format!("cannot remove stale socket {path}"))?; }

        let socket = RawSocket::new(Domain::UNIX, Type::STREAM, None).or_fail("cannot create unix socket")?;
        let address = socket2::SockAddr::unix(path).or_fail_with(|| format!("invalid unix socket path {path}"))?;

        socket.set_nonblocking(true).or_fail("cannot set nonblocking")?;
        socket.bind(&address).or_fail_with(|| format!("cannot bind {path}"))?;
        socket.listen(backlog).or_fail_with(|| format!("cannot listen on {path}"))?;

        Ok(UnixListener::from(socket))

    }

}
