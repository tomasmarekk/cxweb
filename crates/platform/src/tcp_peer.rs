//! OS-observed user identity for an accepted IPv4 loopback connection.
//! A URL capability alone is insufficient when native config is sandbox-readable.
use std::{
    io,
    mem::{offset_of, size_of},
    net::SocketAddr,
    os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle},
    ptr::null_mut,
};
use windows_sys::Win32::{
    Foundation::{ERROR_INSUFFICIENT_BUFFER, FILETIME},
    NetworkManagement::IpHelper::{
        GetExtendedTcpTable, MIB_TCPROW_OWNER_MODULE, MIB_TCPTABLE_OWNER_MODULE,
        TCP_TABLE_OWNER_MODULE_CONNECTIONS,
    },
    Networking::WinSock::AF_INET,
    System::Threading::{GetProcessTimes, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION},
};

fn denied() -> io::Error {
    io::Error::new(io::ErrorKind::PermissionDenied, "E_TCP_PEER")
}

#[derive(Debug, PartialEq, Eq)]
struct Owner {
    pid: u32,
    created: i64,
}

fn owner(peer: SocketAddr, local: SocketAddr) -> io::Result<Owner> {
    let (SocketAddr::V4(peer), SocketAddr::V4(local)) = (peer, local) else {
        return Err(denied());
    };
    if !peer.ip().is_loopback() || !local.ip().is_loopback() {
        return Err(denied());
    }
    let mut bytes = 0u32;
    // SAFETY: the first call only obtains a size. Subsequent storage is aligned
    // to the OS table, bounded, and all row offsets are checked before reads.
    unsafe {
        let status = GetExtendedTcpTable(
            null_mut(),
            &mut bytes,
            0,
            AF_INET as u32,
            TCP_TABLE_OWNER_MODULE_CONNECTIONS,
            0,
        );
        if status != ERROR_INSUFFICIENT_BUFFER {
            return Err(denied());
        }
        for _ in 0..3 {
            if !(8..=16 * 1024 * 1024).contains(&bytes) {
                return Err(denied());
            }
            let mut storage = vec![0u64; (bytes as usize).div_ceil(8)];
            let capacity = storage.len() * 8;
            bytes = capacity as u32;
            let status = GetExtendedTcpTable(
                storage.as_mut_ptr().cast(),
                &mut bytes,
                0,
                AF_INET as u32,
                TCP_TABLE_OWNER_MODULE_CONNECTIONS,
                0,
            );
            if status == ERROR_INSUFFICIENT_BUFFER {
                continue;
            }
            if status != 0 || bytes as usize > capacity {
                return Err(denied());
            }
            let count = *storage.as_ptr().cast::<u32>() as usize;
            let offset = offset_of!(MIB_TCPTABLE_OWNER_MODULE, table);
            if count > 100_000
                || offset + count * size_of::<MIB_TCPROW_OWNER_MODULE>() > bytes as usize
            {
                return Err(denied());
            }
            let mut found = None;
            for index in 0..count {
                let row = &*storage
                    .as_ptr()
                    .cast::<u8>()
                    .add(offset + index * size_of::<MIB_TCPROW_OWNER_MODULE>())
                    .cast::<MIB_TCPROW_OWNER_MODULE>();
                if row.dwLocalAddr == u32::from_ne_bytes(peer.ip().octets())
                    && row.dwRemoteAddr == u32::from_ne_bytes(local.ip().octets())
                    && u16::from_be(row.dwLocalPort as u16) == peer.port()
                    && u16::from_be(row.dwRemotePort as u16) == local.port()
                {
                    if found.is_some() || row.dwOwningPid == 0 || row.liCreateTimestamp <= 0 {
                        return Err(denied());
                    }
                    found = Some(Owner {
                        pid: row.dwOwningPid,
                        created: row.liCreateTimestamp,
                    });
                }
            }
            return found.ok_or_else(denied);
        }
    }
    Err(denied())
}

pub fn verify(peer: SocketAddr, local: SocketAddr) -> io::Result<()> {
    verify_user(peer, local, &crate::state::current_sid()?)
}

fn verify_user(peer: SocketAddr, local: SocketAddr, expected: &str) -> io::Result<()> {
    let before = owner(peer, local)?;
    // SAFETY: query-only access to the OS-reported process, immediately owned.
    // Creation-time comparison excludes a recycled PID; the handle pins identity
    // while the TCP row is checked again. No memory or credentials are read.
    let process = unsafe {
        let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, before.pid);
        if handle.is_null() {
            return Err(denied());
        }
        OwnedHandle::from_raw_handle(handle)
    };
    let mut times = [FILETIME::default(); 4];
    // SAFETY: the live process handle and four distinct writable FILETIMEs.
    let ok = unsafe {
        GetProcessTimes(
            process.as_raw_handle(),
            &mut times[0],
            &mut times[1],
            &mut times[2],
            &mut times[3],
        )
    };
    let created = ((times[0].dwHighDateTime as u64) << 32) | times[0].dwLowDateTime as u64;
    if ok == 0
        || created == 0
        || created > before.created as u64
        || crate::state::sid_for_process(process.as_raw_handle())? != expected
        || owner(peer, local)? != before
    {
        return Err(denied());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn actual_loopback_owner_is_accepted_and_mismatched_identity_is_denied() {
        let listener = crate::loopback::bind(0).unwrap();
        let local = listener.local_addr().unwrap();
        let client = tokio::net::TcpStream::connect(local).await.unwrap();
        let (_server, peer) = listener.accept().await.unwrap();
        assert_eq!(owner(peer, local).unwrap().pid, std::process::id());
        verify(peer, local).unwrap();
        assert!(verify_user(peer, local, "S-1-0-0").is_err());
        assert!(verify(local, peer).is_ok()); // Both sockets belong to this process.
        assert!(verify("192.0.2.1:1234".parse().unwrap(), local).is_err());
        drop(client);
    }

    #[test]
    #[ignore = "test helper started only by another_process_is_identified_by_its_actual_connection"]
    fn tcp_peer_child() {
        use std::io::Read;
        let address = std::env::var("CXWEB_PEER_TEST_SOCKET").unwrap();
        let mut stream = std::net::TcpStream::connect(address).unwrap();
        stream
            .set_read_timeout(Some(std::time::Duration::from_secs(10)))
            .unwrap();
        stream.read_exact(&mut [0u8; 1]).unwrap();
    }

    #[tokio::test]
    async fn another_process_is_identified_by_its_actual_connection() {
        use tokio::io::AsyncWriteExt;
        let listener = crate::loopback::bind(0).unwrap();
        let local = listener.local_addr().unwrap();
        let mut command = tokio::process::Command::new(std::env::current_exe().unwrap());
        command
            .args(["--ignored", "--exact", "tcp_peer::tests::tcp_peer_child"])
            .env("CXWEB_PEER_TEST_SOCKET", local.to_string())
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .creation_flags(0x08000000)
            .kill_on_drop(true);
        let mut child = command.spawn().unwrap();
        let (mut stream, peer) =
            tokio::time::timeout(std::time::Duration::from_secs(10), listener.accept())
                .await
                .unwrap()
                .unwrap();
        assert_eq!(owner(peer, local).unwrap().pid, child.id().unwrap());
        verify(peer, local).unwrap();
        assert!(verify_user(peer, local, "S-1-0-0").is_err());
        stream.write_all(&[1]).await.unwrap();
        assert!(child.wait().await.unwrap().success());
    }
}
