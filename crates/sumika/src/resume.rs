use std::env;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

pub fn dir() -> PathBuf {
    dir_from(
        env::var_os("SUMIKA_STATE_DIR"),
        env::var_os("XDG_STATE_HOME"),
        env::var_os("HOME"),
    )
}

pub fn dir_from(
    state_dir: Option<std::ffi::OsString>,
    xdg_state_home: Option<std::ffi::OsString>,
    home: Option<std::ffi::OsString>,
) -> PathBuf {
    if let Some(dir) = state_dir.filter(|dir| !dir.is_empty()) {
        return PathBuf::from(dir).join("resume");
    }
    if let Some(dir) = xdg_state_home.filter(|dir| !dir.is_empty()) {
        return PathBuf::from(dir).join("sumika/resume");
    }
    match home.filter(|dir| !dir.is_empty()) {
        Some(home) => PathBuf::from(home).join(".local/state/sumika/resume"),
        None => PathBuf::from("sumika-resume"),
    }
}

pub fn remember(name: &str, hint: &str) -> io::Result<()> {
    if !valid_name(name) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "invalid session name",
        ));
    }
    remember_at(&dir().join(name), hint)
}

fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 64
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

fn remember_at(path: &Path, hint: &str) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = fs::set_permissions(parent, fs::Permissions::from_mode(0o700));
        }
    }
    fs::write(path, format!("{hint}\n"))
}

pub fn recall(name: &str) -> io::Result<Option<String>> {
    if !valid_name(name) {
        return Ok(None);
    }
    match fs::read_to_string(dir().join(name)) {
        Ok(text) => {
            let hint = text.trim();
            if hint.is_empty() {
                Ok(None)
            } else {
                Ok(Some(hint.to_string()))
            }
        }
        Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(err) => Err(err),
    }
}

pub fn apply(argv: Vec<String>, hint: &str) -> Vec<String> {
    if hint.is_empty() || argv.is_empty() {
        return argv;
    }
    if already_resumed(&argv) {
        return argv;
    }
    let bin = Path::new(&argv[0])
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("");
    match bin {
        "claude" | "grok" => {
            let mut out = argv;
            out.push("--resume".into());
            out.push(hint.into());
            out
        }
        "codex" => {
            let mut out = vec![argv[0].clone(), "resume".into(), hint.into()];
            out.extend(argv.into_iter().skip(1));
            out
        }
        _ => argv,
    }
}

fn already_resumed(argv: &[String]) -> bool {
    argv.windows(2)
        .any(|pair| pair[0] == "--resume" || pair[0] == "-r")
        || argv.get(1).is_some_and(|arg| arg == "resume")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn claude_and_grok_append_resume_id() {
        assert_eq!(
            apply(vec!["claude".into()], "abc"),
            vec!["claude", "--resume", "abc"]
        );
        assert_eq!(
            apply(vec!["/opt/grok".into(), "--foo".into()], "sid"),
            vec!["/opt/grok", "--foo", "--resume", "sid"]
        );
    }

    #[test]
    fn codex_uses_resume_subcommand() {
        assert_eq!(
            apply(vec!["codex".into()], "uuid"),
            vec!["codex", "resume", "uuid"]
        );
    }

    #[test]
    fn kiro_and_unknown_keep_argv() {
        assert_eq!(apply(vec!["kiro-cli".into()], "nope"), vec!["kiro-cli"]);
        assert_eq!(apply(vec!["pi".into()], "nope"), vec!["pi"]);
        assert_eq!(apply(vec!["unknown".into()], "nope"), vec!["unknown"]);
    }
}
