use std::env;
use std::ffi::OsString;
use std::path::PathBuf;

pub fn path() -> PathBuf {
    path_from(
        env::var_os("SUMIKA_LOG"),
        env::var_os("SUMIKA_STATE_DIR"),
        env::var_os("XDG_STATE_HOME"),
        env::var_os("HOME"),
    )
}

pub fn explicit_path() -> bool {
    env::var_os("SUMIKA_LOG").is_some_and(|path| !path.is_empty())
}

pub fn default_state_parent() -> bool {
    !explicit_path() && env::var_os("SUMIKA_STATE_DIR").is_none_or(|dir| dir.is_empty())
}

pub fn path_from(
    log: Option<OsString>,
    state_dir: Option<OsString>,
    xdg_state_home: Option<OsString>,
    home: Option<OsString>,
) -> PathBuf {
    if let Some(path) = log.filter(|path| !path.is_empty()) {
        return PathBuf::from(path);
    }
    if let Some(dir) = state_dir.filter(|dir| !dir.is_empty()) {
        return PathBuf::from(dir).join("daemon.log");
    }
    if let Some(dir) = xdg_state_home.filter(|dir| !dir.is_empty()) {
        return PathBuf::from(dir).join("sumika/daemon.log");
    }
    match home.filter(|dir| !dir.is_empty()) {
        Some(home) => PathBuf::from(home).join(".local/state/sumika/daemon.log"),
        None => PathBuf::from("sumika-daemon.log"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explicit_log_wins() {
        let path = path_from(
            Some("/tmp/sumika.log".into()),
            Some("/state".into()),
            Some("/xdg".into()),
            None,
        );
        assert_eq!(path, PathBuf::from("/tmp/sumika.log"));
    }

    #[test]
    fn default_is_xdg_state_home() {
        let path = path_from(None, None, None, Some("/home/op".into()));
        assert_eq!(
            path,
            PathBuf::from("/home/op/.local/state/sumika/daemon.log")
        );
    }
}
