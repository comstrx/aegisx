use super::arch::Sys;

impl Sys {

    pub fn notify ( state: &str ) {

        #[cfg(target_os = "linux")]
        {

            use std::os::linux::net::SocketAddrExt;
            use std::os::unix::net::{SocketAddr, UnixDatagram};

            let Some(path) = std::env::var_os("NOTIFY_SOCKET") else { return; };

            let address = match path.as_encoded_bytes().strip_prefix(b"@") {
                Some(name) => SocketAddr::from_abstract_name(name),
                None => SocketAddr::from_pathname(&path),
            };

            if let ( Ok(address), Ok(socket) ) = ( address, UnixDatagram::unbound() ) { let _ = socket.send_to_addr(state.as_bytes(), &address); }

        }

        #[cfg(not(target_os = "linux"))]
        let _ = state;

    }

}
