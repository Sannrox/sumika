use std::path::{Path, PathBuf};
#[cfg(target_os = "macos")]
use std::process::Command;

use serde::Serialize;
use sumika_protocol::SessionInfo;

#[derive(Debug, Serialize)]
pub struct DoctorSession {
    pub name: String,
    pub pid: Option<u32>,
    pub status: String,
}

#[derive(Debug, Serialize)]
pub struct DoctorReport {
    pub socket: String,
    pub reachable: bool,
    pub launchd_loaded: Option<bool>,
    pub config: String,
    pub sessions: Vec<DoctorSession>,
}

impl DoctorReport {
    pub fn from_parts(
        socket: PathBuf,
        config: PathBuf,
        reachable: bool,
        sessions: Vec<SessionInfo>,
    ) -> Self {
        Self {
            socket: socket.display().to_string(),
            reachable,
            launchd_loaded: launchd_loaded(),
            config: config.display().to_string(),
            sessions: sessions
                .into_iter()
                .map(|session| DoctorSession {
                    name: session.name,
                    pid: session.pid,
                    status: match session.status {
                        sumika_protocol::Status::Running => "running",
                        sumika_protocol::Status::Idle => "idle",
                        sumika_protocol::Status::Blocked => "blocked",
                        sumika_protocol::Status::Dead => "dead",
                        sumika_protocol::Status::Unknown => "unknown",
                    }
                    .into(),
                })
                .collect(),
        }
    }
}

pub const LAUNCHD_LABEL: &str = "com.sumika.daemon";

pub fn launchd_plist_path() -> Option<PathBuf> {
    let home = std::env::var_os("HOME")?;
    Some(PathBuf::from(home).join("Library/LaunchAgents/com.sumika.daemon.plist"))
}

pub fn launchd_loaded() -> Option<bool> {
    #[cfg(target_os = "macos")]
    {
        let uid = unsafe { libc::getuid() };
        let target = format!("gui/{uid}/{LAUNCHD_LABEL}");
        let status = Command::new("launchctl")
            .args(["print", &target])
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .ok()?;
        Some(status.success())
    }
    #[cfg(not(target_os = "macos"))]
    {
        None
    }
}

pub fn install_launchd(bin: &Path) -> Result<PathBuf, String> {
    let plist_path = launchd_plist_path().ok_or_else(|| "HOME is unset".to_string())?;
    if let Some(parent) = plist_path.parent() {
        std::fs::create_dir_all(parent).map_err(|err| err.to_string())?;
    }
    let template = include_str!("../../../contrib/launchd/com.sumika.daemon.plist");
    let body = template.replace("SUMIKA_BIN", &xml_escape(&bin.display().to_string()));
    std::fs::write(&plist_path, body).map_err(|err| err.to_string())?;
    Ok(plist_path)
}

pub fn bootstrap_launchd(plist: &Path) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        let uid = unsafe { libc::getuid() };
        let domain = format!("gui/{uid}");
        let _ = Command::new("launchctl")
            .args(["bootout", &domain, &plist.display().to_string()])
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status();
        let status = Command::new("launchctl")
            .args(["bootstrap", &domain, &plist.display().to_string()])
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::piped())
            .output()
            .map_err(|err| err.to_string())?;
        if status.status.success() {
            return Ok(());
        }
        Err(String::from_utf8_lossy(&status.stderr).trim().to_string())
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = plist;
        Err("launchd is only available on macOS".into())
    }
}

fn xml_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plist_template_mentions_the_label() {
        let template = include_str!("../../../contrib/launchd/com.sumika.daemon.plist");
        assert!(template.contains(LAUNCHD_LABEL));
        assert!(template.contains("SUMIKA_BIN"));
        assert!(template.contains("daemon"));
    }

    #[test]
    fn systemd_unit_starts_the_daemon() {
        let unit = include_str!("../../../contrib/systemd/sumika.service");
        assert!(unit.contains("ExecStart=sumika daemon"));
        assert!(unit.contains("WantedBy=default.target"));
    }
}
