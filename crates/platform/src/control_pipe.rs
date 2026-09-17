//! Per-user local control transport. No TCP port, remote clients or permissive
//! creation window. Same-user native processes remain outside the threat boundary.
use std::{io, mem::size_of, os::windows::io::AsRawHandle};
use tokio::{
    io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt},
    net::windows::named_pipe::{ClientOptions, NamedPipeClient, NamedPipeServer, ServerOptions},
};
use windows_sys::Win32::{
    Foundation::ERROR_PIPE_BUSY,
    Security::SECURITY_ATTRIBUTES,
    Storage::FileSystem::SECURITY_IDENTIFICATION,
    System::Pipes::{GetNamedPipeClientProcessId, GetNamedPipeServerProcessId},
};

const FRAME_LIMIT: usize = 64 * 1024;

fn name(installation: &str) -> io::Result<String> {
    if installation.len() != 32 || !installation.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(io::Error::other("E_CONTROL_ADDRESS"));
    }
    Ok(format!(
        r"\\.\pipe\cxweb-{}-{}",
        crate::state::current_sid()?,
        installation.to_ascii_lowercase()
    ))
}

pub struct ControlListener {
    installation: String,
    pending: NamedPipeServer,
}

/// The pending instance retains name ownership between clients. Each accepted
/// connection gets a fresh I/O object: disconnect/reconnect on one Mio pipe can
/// retain a completed read error from the preceding client.
pub fn listen(installation: &str) -> io::Result<ControlListener> {
    Ok(ControlListener {
        installation: installation.into(),
        pending: create(installation, true)?,
    })
}

impl ControlListener {
    /// One handler at a time. A fresh pending instance is created before the
    /// connected instance is handed off or dropped, leaving no ownership gap.
    pub async fn accept(&mut self) -> io::Result<NamedPipeServer> {
        self.pending.connect().await?;
        let peer = verify_client(&self.pending);
        // An overlapped read cancelled by dropping the preceding I/O object may
        // retain its kernel instance until the completion is processed. Yield
        // while retaining the current instance; never close and rebind the name.
        let next = tokio::time::timeout(std::time::Duration::from_secs(1), async {
            loop {
                match create(&self.installation, false) {
                    Err(error) if error.raw_os_error() == Some(ERROR_PIPE_BUSY as i32) => {
                        tokio::time::sleep(std::time::Duration::from_millis(2)).await;
                    }
                    result => break result,
                }
            }
        })
        .await
        .map_err(|_| io::Error::new(io::ErrorKind::TimedOut, "E_CONTROL_BUSY"))??;
        let connected = std::mem::replace(&mut self.pending, next);
        peer?;
        Ok(connected)
    }
}

fn create(installation: &str, first: bool) -> io::Result<NamedPipeServer> {
    let descriptor = crate::state::descriptor()?;
    let mut attributes = SECURITY_ATTRIBUTES {
        nLength: size_of::<SECURITY_ATTRIBUTES>() as u32,
        lpSecurityDescriptor: descriptor.0,
        bInheritHandle: 0,
    };
    // SAFETY: attributes and its descriptor stay alive through CreateNamedPipe;
    // Windows captures the descriptor at creation. Tokio owns the returned handle.
    unsafe {
        ServerOptions::new()
            .first_pipe_instance(first)
            .reject_remote_clients(true)
            .max_instances(2)
            .in_buffer_size(FRAME_LIMIT as u32)
            .out_buffer_size(FRAME_LIMIT as u32)
            .create_with_security_attributes_raw(
                name(installation)?,
                (&mut attributes as *mut SECURITY_ATTRIBUTES).cast(),
            )
    }
}

/// Verify before reading or acting on a connected request. The named-pipe DACL
/// is the first boundary; this additionally checks the OS-reported peer user.
pub fn verify_client(server: &NamedPipeServer) -> io::Result<()> {
    let mut pid = 0;
    // SAFETY: live server pipe handle and writable process-id output.
    if unsafe { GetNamedPipeClientProcessId(server.as_raw_handle(), &mut pid) } == 0 {
        return Err(io::Error::last_os_error());
    }
    crate::state::verify_process_user(pid)
}

pub fn connect(installation: &str) -> io::Result<NamedPipeClient> {
    // Identification permits identity inspection, not impersonating the client
    // to open resources. Never use the default impersonation-level pipe open.
    let client = ClientOptions::new()
        .security_qos_flags(SECURITY_IDENTIFICATION)
        .open(name(installation)?)?;
    let mut pid = 0;
    // SAFETY: live client pipe handle and writable process-id output.
    if unsafe { GetNamedPipeServerProcessId(client.as_raw_handle(), &mut pid) } == 0 {
        return Err(io::Error::last_os_error());
    }
    crate::state::verify_process_user(pid)?;
    Ok(client)
}

/// Frames are bounded before allocation. The typed protocol owns decoding and
/// operation authorization; callers must bound the complete exchange by time.
pub async fn read_frame(stream: &mut (impl AsyncRead + Unpin)) -> io::Result<Vec<u8>> {
    let length = stream.read_u32_le().await? as usize;
    if length == 0 || length > FRAME_LIMIT {
        return Err(io::Error::other("E_CONTROL_FRAME"));
    }
    let mut bytes = vec![0; length];
    stream.read_exact(&mut bytes).await?;
    Ok(bytes)
}

pub async fn write_frame(stream: &mut (impl AsyncWrite + Unpin), bytes: &[u8]) -> io::Result<()> {
    if bytes.is_empty() || bytes.len() > FRAME_LIMIT {
        return Err(io::Error::other("E_CONTROL_FRAME"));
    }
    stream.write_u32_le(bytes.len() as u32).await?;
    stream.write_all(bytes).await?;
    stream.flush().await
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, SystemTime, UNIX_EPOCH};
    fn id() -> String {
        format!(
            "{:032x}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        )
    }

    #[tokio::test]
    async fn private_pipe_verifies_peers_and_reuses_its_name_without_ownership_gap() {
        let installation = id();
        let mut listener = listen(&installation).unwrap();
        assert!(listen(&installation).is_err());
        for body in [b"first".as_slice(), b"second".as_slice()]
            .into_iter()
            .cycle()
            .take(32)
        {
            let mut client = connect(&installation).unwrap();
            let mut server = tokio::time::timeout(Duration::from_secs(1), listener.accept())
                .await
                .unwrap()
                .unwrap();
            verify_client(&server).unwrap();
            write_frame(&mut client, body).await.unwrap();
            assert_eq!(read_frame(&mut server).await.unwrap(), body);
            write_frame(&mut server, b"ack").await.unwrap();
            assert_eq!(read_frame(&mut client).await.unwrap(), b"ack");
            drop(client);
            drop(server);
            assert!(listen(&installation).is_err());
        }
    }

    #[tokio::test]
    async fn truncated_empty_and_oversized_frames_are_rejected() {
        for length in [0u32, FRAME_LIMIT as u32 + 1] {
            let input = length.to_le_bytes();
            assert!(read_frame(&mut input.as_slice()).await.is_err());
        }
        let mut truncated = &b"\x05\0\0\0ab"[..];
        assert!(read_frame(&mut truncated).await.is_err());
        let mut output = Vec::new();
        assert!(
            write_frame(&mut output, &vec![0; FRAME_LIMIT + 1])
                .await
                .is_err()
        );
        assert!(output.is_empty());
        for invalid in [
            "",
            "../outside",
            r"\\server\pipe\other",
            "zzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzz",
        ] {
            assert!(listen(invalid).is_err());
        }
    }

    #[tokio::test]
    async fn pipe_is_created_with_exact_private_owner_and_dacl() {
        use windows_sys::Win32::Security::{
            Authorization::{GetSecurityInfo, SE_KERNEL_OBJECT},
            DACL_SECURITY_INFORMATION, OWNER_SECURITY_INFORMATION,
        };
        let server = listen(&id()).unwrap();
        let mut raw = std::ptr::null_mut();
        // SAFETY: live pipe handle and writable security-descriptor output;
        // LocalAllocation frees the descriptor returned by GetSecurityInfo.
        let result = unsafe {
            GetSecurityInfo(
                server.pending.as_raw_handle(),
                SE_KERNEL_OBJECT,
                OWNER_SECURITY_INFORMATION | DACL_SECURITY_INFORMATION,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                &mut raw,
            )
        };
        assert_eq!(result, 0);
        let actual = crate::state::LocalAllocation(raw);
        let expected = crate::state::descriptor().unwrap();
        assert_eq!(
            crate::state::descriptor_text(actual.0).unwrap(),
            crate::state::descriptor_text(expected.0).unwrap()
        );
    }
}
