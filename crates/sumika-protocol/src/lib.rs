use std::env;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

pub const EXIT_OK: i32 = 0;
pub const EXIT_USAGE: i32 = 1;
pub const EXIT_UNREACHABLE: i32 = 2;
pub const EXIT_UNKNOWN_SESSION: i32 = 3;
pub const EXIT_SPAWN_FAILED: i32 = 4;
pub const EXIT_STOLEN: i32 = 5;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    Running,
    Idle,
    Blocked,
    Dead,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionInfo {
    pub name: String,
    pub argv: Vec<String>,
    pub cwd: String,
    pub status: Status,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pid: Option<u32>,
    pub focused: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Request {
    Ping,
    Start {
        name: String,
        argv: Vec<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        cwd: Option<String>,
    },
    List,
    Attach {
        name: String,
    },
    Resize {
        name: String,
        cols: u16,
        rows: u16,
    },
    Kill {
        name: String,
        #[serde(default)]
        force: bool,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    UnknownSession,
    SpawnFailed,
    InvalidRequest,
    Dead,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ErrorBody {
    pub code: ErrorCode,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Response {
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<ErrorBody>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session: Option<SessionInfo>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sessions: Option<Vec<SessionInfo>>,
}

impl Response {
    pub fn ok() -> Self {
        Self {
            ok: true,
            error: None,
            session: None,
            sessions: None,
        }
    }

    pub fn session(session: SessionInfo) -> Self {
        Self {
            ok: true,
            error: None,
            session: Some(session),
            sessions: None,
        }
    }

    pub fn sessions(sessions: Vec<SessionInfo>) -> Self {
        Self {
            ok: true,
            error: None,
            session: None,
            sessions: Some(sessions),
        }
    }

    pub fn err(code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            ok: false,
            error: Some(ErrorBody {
                code,
                message: message.into(),
            }),
            session: None,
            sessions: None,
        }
    }

    pub fn exit_code(&self) -> i32 {
        match self.error.as_ref().map(|e| &e.code) {
            None => EXIT_OK,
            Some(ErrorCode::UnknownSession) | Some(ErrorCode::Dead) => EXIT_UNKNOWN_SESSION,
            Some(ErrorCode::SpawnFailed) => EXIT_SPAWN_FAILED,
            Some(ErrorCode::InvalidRequest) => EXIT_USAGE,
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ProtocolError {
    #[error("invalid json: {0}")]
    Json(#[from] serde_json::Error),
    #[error("line is not valid utf-8")]
    Utf8,
}

pub fn encode_line(value: &impl Serialize) -> Result<Vec<u8>, ProtocolError> {
    let mut bytes = serde_json::to_vec(value)?;
    bytes.push(b'\n');
    Ok(bytes)
}

pub fn decode_line<T: for<'de> Deserialize<'de>>(line: &[u8]) -> Result<T, ProtocolError> {
    let line = std::str::from_utf8(line).map_err(|_| ProtocolError::Utf8)?;
    let line = line.trim_end_matches(['\n', '\r']);
    Ok(serde_json::from_str(line)?)
}

pub fn default_socket_path() -> PathBuf {
    if let Ok(path) = env::var("SUMIKA_SOCK")
        && !path.is_empty()
    {
        return PathBuf::from(path);
    }
    socket_dir().join("sumika.sock")
}

pub fn socket_dir() -> PathBuf {
    if let Ok(dir) = env::var("XDG_RUNTIME_DIR")
        && !dir.is_empty()
    {
        return PathBuf::from(dir).join("sumika");
    }
    let home = env::var_os("HOME").map(PathBuf::from);
    #[cfg(target_os = "macos")]
    {
        home.map(|home| home.join("Library/Caches/sumika"))
            .unwrap_or_else(|| env::temp_dir().join(format!("sumika-{}", uid())))
    }
    #[cfg(not(target_os = "macos"))]
    {
        home.map(|home| home.join(".cache/sumika"))
            .unwrap_or_else(|| env::temp_dir().join(format!("sumika-{}", uid())))
    }
}

pub fn prepare_socket_parent(sock: &Path) -> io::Result<()> {
    let parent = socket_parent(sock)?;
    let created = match std::fs::symlink_metadata(&parent) {
        Err(err) if err.kind() == io::ErrorKind::NotFound => {
            std::fs::create_dir_all(&parent)?;
            true
        }
        Err(err) => return Err(err),
        Ok(meta) if meta.file_type().is_symlink() => {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                format!("{} is a symlink", parent.display()),
            ));
        }
        Ok(meta) if !meta.is_dir() => {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                format!("{} is not a directory", parent.display()),
            ));
        }
        Ok(_) => false,
    };
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if !path_owned_by_current_user(&parent) {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                format!("{} is not owned by the current user", parent.display()),
            ));
        }
        if created || parent == socket_dir() {
            std::fs::set_permissions(&parent, std::fs::Permissions::from_mode(0o700))?;
        }
        let mode = std::fs::symlink_metadata(&parent)?.permissions().mode();
        if mode & 0o077 != 0 {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                format!("{} is not private", parent.display()),
            ));
        }
    }
    Ok(())
}

fn socket_parent(sock: &Path) -> io::Result<PathBuf> {
    match sock.parent() {
        None => std::env::current_dir(),
        Some(parent) if parent.as_os_str().is_empty() => std::env::current_dir(),
        Some(parent) if parent.is_relative() => Ok(std::env::current_dir()?.join(parent)),
        Some(parent) => Ok(parent.to_path_buf()),
    }
}

#[cfg(unix)]
pub fn path_owned_by_current_user(path: &Path) -> bool {
    use std::os::unix::fs::MetadataExt;
    std::fs::symlink_metadata(path)
        .map(|meta| meta.uid() == uid() && !meta.file_type().is_symlink())
        .unwrap_or(false)
}

#[cfg(unix)]
pub fn is_unix_socket(path: &Path) -> bool {
    use std::os::unix::fs::FileTypeExt;
    std::fs::symlink_metadata(path)
        .map(|meta| meta.file_type().is_socket())
        .unwrap_or(false)
}

#[cfg(unix)]
fn uid() -> u32 {
    // getuid has no preconditions.
    unsafe { libc::getuid() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn start_request_json_shape() {
        let req = Request::Start {
            name: "demo".into(),
            argv: vec!["bash".into()],
            cwd: None,
        };
        let value = serde_json::to_value(&req).unwrap();
        assert_eq!(value["op"], "start");
        assert_eq!(value["name"], "demo");
        assert_eq!(value["argv"][0], "bash");
        assert!(value.get("cwd").is_none());
    }

    #[test]
    fn attach_then_line_roundtrip() {
        let req = Request::Attach {
            name: "kiro".into(),
        };
        let line = encode_line(&req).unwrap();
        let decoded: Request = decode_line(&line).unwrap();
        assert_eq!(req, decoded);
    }

    #[test]
    fn fallback_socket_dir_is_user_private() {
        let dir = socket_dir();
        if let Ok(runtime) = env::var("XDG_RUNTIME_DIR")
            && !runtime.is_empty()
        {
            assert!(
                dir.starts_with(&runtime),
                "fallback socket dir {dir:?} is not under XDG_RUNTIME_DIR {runtime}"
            );
            return;
        }
        let home = env::var("HOME").expect("HOME");
        assert!(
            dir.starts_with(&home),
            "fallback socket dir {dir:?} is not under {home}"
        );
        assert_ne!(dir, std::path::Path::new("/tmp"));
    }

    #[test]
    fn error_exit_codes() {
        assert_eq!(Response::ok().exit_code(), EXIT_OK);
        assert_eq!(
            Response::err(ErrorCode::UnknownSession, "nope").exit_code(),
            EXIT_UNKNOWN_SESSION
        );
        assert_eq!(
            Response::err(ErrorCode::SpawnFailed, "nope").exit_code(),
            EXIT_SPAWN_FAILED
        );
    }
}
