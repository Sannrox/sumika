use std::path::{Path, PathBuf};
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
    pub systemd_user_loaded: Option<bool>,
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
            systemd_user_loaded: systemd_user_loaded(),
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

pub const SYSTEMD_UNIT: &str = "sumika.service";

pub fn systemd_unit_path() -> Option<PathBuf> {
    systemd_unit_path_from(
        std::env::var_os("XDG_CONFIG_HOME"),
        std::env::var_os("HOME"),
    )
}

pub fn systemd_unit_path_from(
    xdg_config_home: Option<std::ffi::OsString>,
    home: Option<std::ffi::OsString>,
) -> Option<PathBuf> {
    if let Some(dir) = xdg_config_home.filter(|dir| !dir.is_empty()) {
        return Some(PathBuf::from(dir).join("systemd/user").join(SYSTEMD_UNIT));
    }
    let home = home.filter(|dir| !dir.is_empty())?;
    Some(
        PathBuf::from(home)
            .join(".config/systemd/user")
            .join(SYSTEMD_UNIT),
    )
}

pub fn systemd_user_loaded() -> Option<bool> {
    #[cfg(target_os = "linux")]
    {
        let status = Command::new("systemctl")
            .args(["--user", "is-active", SYSTEMD_UNIT])
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .ok()?;
        Some(status.success())
    }
    #[cfg(not(target_os = "linux"))]
    {
        None
    }
}

pub fn install_systemd(bin: &Path) -> Result<PathBuf, String> {
    let unit_path = systemd_unit_path().ok_or_else(|| "HOME is unset".to_string())?;
    write_systemd_unit(&unit_path, bin)?;
    Ok(unit_path)
}

fn write_systemd_unit(unit_path: &Path, bin: &Path) -> Result<(), String> {
    if let Some(parent) = unit_path.parent() {
        std::fs::create_dir_all(parent).map_err(|err| err.to_string())?;
    }
    let template = include_str!("../../../contrib/systemd/sumika.service");
    let body = template.replace("SUMIKA_BIN", &bin.display().to_string());
    std::fs::write(unit_path, body).map_err(|err| err.to_string())
}

pub fn enable_systemd() -> Result<(), String> {
    #[cfg(target_os = "linux")]
    {
        let reload = Command::new("systemctl")
            .args(["--user", "daemon-reload"])
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::piped())
            .output()
            .map_err(|err| err.to_string())?;
        if !reload.status.success() {
            return Err(String::from_utf8_lossy(&reload.stderr).trim().to_string());
        }
        let status = Command::new("systemctl")
            .args(["--user", "enable", "--now", SYSTEMD_UNIT])
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
    #[cfg(not(target_os = "linux"))]
    {
        Err("systemd --user is only available on Linux".into())
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
        assert!(unit.contains("ExecStart=SUMIKA_BIN daemon"));
        assert!(unit.contains("WantedBy=default.target"));
    }

    #[test]
    fn systemd_unit_path_prefers_xdg_config_home() {
        let path = systemd_unit_path_from(Some("/xdg/config".into()), Some("/home/op".into()));
        assert_eq!(
            path,
            Some(PathBuf::from("/xdg/config/systemd/user/sumika.service"))
        );
    }

    #[test]
    fn install_systemd_writes_the_binary() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("systemd/user/sumika.service");
        write_systemd_unit(&path, Path::new("/opt/sumika/sumika")).expect("write");
        let body = std::fs::read_to_string(&path).expect("unit");
        assert!(body.contains("ExecStart=/opt/sumika/sumika daemon"));
        assert!(body.contains("WantedBy=default.target"));
    }
}
