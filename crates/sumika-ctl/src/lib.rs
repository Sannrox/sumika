use std::path::{Path, PathBuf};

use sumika_protocol::{
    EXIT_UNREACHABLE, ProtocolError, Request, Response, decode_line, encode_line,
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::UnixStream;

#[derive(Debug, thiserror::Error)]
pub enum ClientError {
    #[error("daemon unreachable at {}: {source}", path.display())]
    Unreachable {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error(transparent)]
    Protocol(#[from] ProtocolError),
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
}

impl ClientError {
    pub fn exit_code(&self) -> i32 {
        match self {
            ClientError::Unreachable { .. } => EXIT_UNREACHABLE,
            ClientError::Protocol(_) | ClientError::Io(_) => EXIT_UNREACHABLE,
        }
    }
}

#[derive(Clone)]
pub struct Client {
    sock: PathBuf,
}

impl Client {
    pub fn new(sock: impl Into<PathBuf>) -> Self {
        Self { sock: sock.into() }
    }

    pub fn sock_path(&self) -> &Path {
        &self.sock
    }

    pub async fn rpc(&self, request: &Request) -> Result<Response, ClientError> {
        let mut stream = self.connect().await?;
        write_line(&mut stream, request).await?;
        read_response(&mut stream).await
    }

    pub async fn attach(&self, name: &str) -> Result<(Response, UnixStream), ClientError> {
        let mut stream = self.connect().await?;
        write_line(
            &mut stream,
            &Request::Attach {
                name: name.to_string(),
            },
        )
        .await?;
        let response = read_response(&mut stream).await?;
        Ok((response, stream))
    }

    async fn connect(&self) -> Result<UnixStream, ClientError> {
        #[cfg(unix)]
        if self.sock.exists() && !sumika_protocol::path_owned_by_current_user(&self.sock) {
            return Err(ClientError::Unreachable {
                path: self.sock.clone(),
                source: std::io::Error::new(
                    std::io::ErrorKind::PermissionDenied,
                    "socket is not owned by the current user",
                ),
            });
        }
        UnixStream::connect(&self.sock)
            .await
            .map_err(|source| ClientError::Unreachable {
                path: self.sock.clone(),
                source,
            })
    }
}

async fn write_line(stream: &mut UnixStream, request: &Request) -> Result<(), ClientError> {
    let bytes = encode_line(request)?;
    stream.write_all(&bytes).await?;
    stream.flush().await?;
    Ok(())
}

async fn read_response(stream: &mut UnixStream) -> Result<Response, ClientError> {
    let mut buf = Vec::new();
    let mut byte = [0u8; 1];
    loop {
        stream.read_exact(&mut byte).await?;
        if byte[0] == b'\n' {
            break;
        }
        buf.push(byte[0]);
        if buf.len() > 8 * 1024 * 1024 {
            return Err(ClientError::Io(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "response line too long",
            )));
        }
    }
    Ok(decode_line(&buf)?)
}
