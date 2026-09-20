mod frame;
mod supervisor;

use std::path::PathBuf;

use std::os::unix::io::AsRawFd;

use anyhow::Context;
use sumika_protocol::{Request, Response, decode_line, encode_line, prepare_socket_parent};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{UnixListener, UnixStream};

use crate::supervisor::Supervisor;

pub async fn run(sock: PathBuf) -> anyhow::Result<()> {
    if sock.exists() {
        #[cfg(unix)]
        {
            if !sumika_protocol::path_owned_by_current_user(&sock) {
                anyhow::bail!(
                    "refusing to use {} — not owned by the current user",
                    sock.display()
                );
            }
            if !sumika_protocol::is_unix_socket(&sock) {
                anyhow::bail!("refusing to replace non-socket path {}", sock.display());
            }
        }
        if UnixStream::connect(&sock).await.is_ok() {
            anyhow::bail!("daemon already running at {}", sock.display());
        }
        std::fs::remove_file(&sock)?;
    }
    prepare_socket_parent(&sock)?;
    let listener = UnixListener::bind(&sock).with_context(|| format!("bind {}", sock.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&sock, std::fs::Permissions::from_mode(0o600))?;
    }
    tracing::info!(path = %sock.display(), "listening");
    let supervisor = Supervisor::new();
    loop {
        match listener.accept().await {
            Ok((stream, _)) => {
                let supervisor = supervisor.clone();
                tokio::spawn(async move {
                    if let Err(err) = handle_connection(supervisor, stream).await {
                        tracing::debug!(error = %err, "connection ended");
                    }
                });
            }
            Err(err) if transient_accept(&err) => {
                tracing::warn!(error = %err, "accept failed");
                tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            }
            Err(err) => return Err(err.into()),
        }
    }
}

fn peer_is_current_user(stream: &UnixStream) -> bool {
    match peer_uid(stream) {
        Some(uid) => uid == unsafe { libc::getuid() },
        None => false,
    }
}

fn peer_uid(stream: &UnixStream) -> Option<u32> {
    let fd = stream.as_raw_fd();
    #[cfg(any(
        target_os = "macos",
        target_os = "ios",
        target_os = "freebsd",
        target_os = "openbsd",
        target_os = "netbsd",
        target_os = "dragonfly"
    ))]
    {
        let mut uid = 0;
        let mut gid = 0;
        let rc = unsafe { libc::getpeereid(fd, &mut uid, &mut gid) };
        if rc == 0 { Some(uid) } else { None }
    }
    #[cfg(target_os = "linux")]
    {
        let mut cred = libc::ucred {
            pid: 0,
            uid: 0,
            gid: 0,
        };
        let mut len = std::mem::size_of::<libc::ucred>() as libc::socklen_t;
        let rc = unsafe {
            libc::getsockopt(
                fd,
                libc::SOL_SOCKET,
                libc::SO_PEERCRED,
                &mut cred as *mut _ as *mut libc::c_void,
                &mut len,
            )
        };
        if rc == 0 { Some(cred.uid) } else { None }
    }
    #[cfg(not(any(
        target_os = "linux",
        target_os = "macos",
        target_os = "ios",
        target_os = "freebsd",
        target_os = "openbsd",
        target_os = "netbsd",
        target_os = "dragonfly"
    )))]
    {
        let _ = fd;
        None
    }
}

fn transient_accept(err: &std::io::Error) -> bool {
    matches!(
        err.kind(),
        std::io::ErrorKind::ConnectionAborted
            | std::io::ErrorKind::Interrupted
            | std::io::ErrorKind::WouldBlock
            | std::io::ErrorKind::TimedOut
    ) || matches!(
        err.raw_os_error(),
        Some(libc::EMFILE) | Some(libc::ENFILE) | Some(libc::ECONNABORTED)
    )
}

async fn handle_connection(supervisor: Supervisor, mut stream: UnixStream) -> anyhow::Result<()> {
    anyhow::ensure!(
        peer_is_current_user(&stream),
        "peer is not the current user"
    );
    let request = read_line(&mut stream).await?;
    match request {
        Request::Attach { name } => match supervisor.attach(&name) {
            Ok(slot) => {
                write_line(&mut stream, &Response::session(slot.info.clone())).await?;
                proxy_attach(slot, stream).await;
            }
            Err((code, message)) => {
                write_line(&mut stream, &Response::err(code, message)).await?;
            }
        },
        other => {
            let response = supervisor.dispatch(other);
            write_line(&mut stream, &response).await?;
        }
    }
    Ok(())
}

async fn proxy_attach(mut slot: supervisor::AttachSlot, stream: UnixStream) {
    let (mut reader, mut writer) = stream.into_split();
    let input = slot.input();
    let mut output = slot.output.take().expect("output");
    let to_pty = async {
        let mut buf = [0u8; 4096];
        loop {
            match reader.read(&mut buf).await {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    if input.send(buf[..n].to_vec()).await.is_err() {
                        break;
                    }
                }
            }
        }
    };
    let to_client = async {
        while let Some(bytes) = output.recv().await {
            if writer.write_all(&bytes).await.is_err() {
                break;
            }
        }
    };
    tokio::select! {
        _ = &mut slot.cancel => {
            tracing::debug!(session = %slot.info.name, "attach stolen");
        }
        _ = to_pty => {}
        _ = to_client => {}
    }
}

async fn read_line(stream: &mut UnixStream) -> anyhow::Result<Request> {
    let mut buf = Vec::new();
    let mut byte = [0u8; 1];
    loop {
        stream.read_exact(&mut byte).await?;
        if byte[0] == b'\n' {
            break;
        }
        buf.push(byte[0]);
        anyhow::ensure!(buf.len() <= 1024 * 1024, "request line too long");
    }
    Ok(decode_line(&buf)?)
}

async fn write_line(stream: &mut UnixStream, response: &Response) -> anyhow::Result<()> {
    let bytes = encode_line(response)?;
    stream.write_all(&bytes).await?;
    stream.flush().await?;
    Ok(())
}
