use std::env;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

pub fn last_path() -> PathBuf {
    last_path_from(
        env::var_os("SUMIKA_STATE_DIR"),
        env::var_os("XDG_STATE_HOME"),
        env::var_os("HOME"),
    )
}

pub fn last_path_from(
    state_dir: Option<std::ffi::OsString>,
    xdg_state_home: Option<std::ffi::OsString>,
    home: Option<std::ffi::OsString>,
) -> PathBuf {
    if let Some(dir) = state_dir.filter(|dir| !dir.is_empty()) {
        return PathBuf::from(dir).join("last");
    }
    if let Some(dir) = xdg_state_home.filter(|dir| !dir.is_empty()) {
        return PathBuf::from(dir).join("sumika/last");
    }
    match home.filter(|dir| !dir.is_empty()) {
        Some(home) => PathBuf::from(home).join(".local/state/sumika/last"),
        None => PathBuf::from("sumika-last"),
    }
}

pub fn remember(name: &str) -> io::Result<()> {
    remember_at(&last_path(), name)
}

fn remember_at(path: &Path, name: &str) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = fs::set_permissions(parent, fs::Permissions::from_mode(0o700));
        }
    }
    fs::write(path, format!("{name}\n"))
}

pub fn recall() -> io::Result<Option<String>> {
    match fs::read_to_string(last_path()) {
        Ok(text) => {
            let name = text.trim();
            if name.is_empty() {
                Ok(None)
            } else {
                Ok(Some(name.to_string()))
            }
        }
        Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(err) => Err(err),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prefers_sumika_state_dir() {
        let path = last_path_from(Some("/var/sumika-state".into()), Some("/xdg".into()), None);
        assert_eq!(path, PathBuf::from("/var/sumika-state/last"));
    }

    #[test]
    fn default_is_xdg_state_home() {
        let path = last_path_from(None, None, Some("/home/op".into()));
        assert_eq!(path, PathBuf::from("/home/op/.local/state/sumika/last"));
    }
}
