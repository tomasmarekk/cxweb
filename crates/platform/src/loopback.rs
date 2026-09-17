//! Exclusive IPv4 loopback ownership for the native compatibility listener.
use std::{
    io,
    mem::size_of,
    net::{Ipv4Addr, SocketAddr},
    os::windows::io::AsRawSocket,
};
use tokio::net::{TcpListener, TcpSocket};
use windows_sys::Win32::Networking::WinSock::{
    SO_EXCLUSIVEADDRUSE, SOL_SOCKET, WSAGetLastError, setsockopt,
};

pub fn bind(port: u16) -> io::Result<TcpListener> {
    let socket = TcpSocket::new_v4()?;
    let enabled: i32 = 1;
    // SAFETY: live Winsock socket and correctly sized BOOL option value. The
    // option is applied before bind; the socket remains owned by Tokio.
    if unsafe {
        setsockopt(
            socket.as_raw_socket() as usize,
            SOL_SOCKET,
            SO_EXCLUSIVEADDRUSE,
            (&enabled as *const i32).cast(),
            size_of::<i32>() as i32,
        )
    } != 0
    {
        // SAFETY: reads this thread's Winsock error immediately after failure.
        return Err(io::Error::from_raw_os_error(unsafe { WSAGetLastError() }));
    }
    socket.bind(SocketAddr::from((Ipv4Addr::LOCALHOST, port)))?;
    socket.listen(128)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn loopback_owner_refuses_even_reuse_enabled_competitor() {
        let listener = bind(0).unwrap();
        let address = listener.local_addr().unwrap();
        assert!(address.ip().is_loopback());
        let competitor = TcpSocket::new_v4().unwrap();
        competitor.set_reuseaddr(true).unwrap();
        assert!(competitor.bind(address).is_err());
        assert!(bind(address.port()).is_err());
        drop(listener);
        assert!(bind(address.port()).is_ok());
    }
}
