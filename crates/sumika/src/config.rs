use std::collections::{HashMap, HashSet};
use std::env;
use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::Deserialize;

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub sessions: Vec<SessionSpec>,
    #[serde(default)]
    pub detach_chord: Option<Vec<String>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct SessionSpec {
    pub name: String,
    pub argv: Vec<String>,
    #[serde(default)]
    pub cwd: Option<String>,
    #[serde(default)]
    pub key: Option<String>,
    #[serde(default)]
    pub project: Option<String>,
}

#[derive(Debug)]
pub enum ConfigError {
    Io {
        path: PathBuf,
        source: io::Error,
    },
    Parse {
        path: PathBuf,
        source: toml::de::Error,
    },
    Invalid(String),
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io { path, source } => write!(f, "read {}: {source}", path.display()),
            Self::Parse { path, source } => write!(f, "parse {}: {source}", path.display()),
            Self::Invalid(message) => write!(f, "{message}"),
        }
    }
}

impl std::error::Error for ConfigError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io { source, .. } => Some(source),
            Self::Parse { source, .. } => Some(source),
            Self::Invalid(_) => None,
        }
    }
}

pub fn resolve_config_path(explicit: Option<PathBuf>) -> PathBuf {
    explicit.unwrap_or_else(default_config_path)
}

pub fn default_config_path() -> PathBuf {
    default_config_path_from(env::var_os("XDG_CONFIG_HOME"), env::var_os("HOME"))
}

pub fn default_config_path_from(
    xdg_config_home: Option<std::ffi::OsString>,
    home: Option<std::ffi::OsString>,
) -> PathBuf {
    if let Some(dir) = xdg_config_home.filter(|dir| !dir.is_empty()) {
        return PathBuf::from(dir).join("sumika").join("config.toml");
    }
    match home.filter(|dir| !dir.is_empty()) {
        Some(home) => PathBuf::from(home).join(".config/sumika/config.toml"),
        None => PathBuf::from("sumika.toml"),
    }
}

pub fn load(path: &Path) -> Result<Config, ConfigError> {
    let text = fs::read_to_string(path).map_err(|source| ConfigError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    let config: Config = toml::from_str(&text).map_err(|source| ConfigError::Parse {
        path: path.to_path_buf(),
        source,
    })?;
    config.validate()?;
    Ok(config)
}

impl Config {
    pub fn lookup(&self, name: &str) -> Option<&SessionSpec> {
        self.sessions.iter().find(|session| session.name == name)
    }

    pub fn detach_chord(&self) -> Result<crate::chord::Chord, ConfigError> {
        match &self.detach_chord {
            None => Ok(crate::chord::Chord::default()),
            Some(keys) => crate::chord::parse_chord(keys).map_err(ConfigError::Invalid),
        }
    }

    pub fn jump_keys(&self) -> HashMap<char, String> {
        let mut jumps = HashMap::new();
        for session in &self.sessions {
            let Some(key) = session.key.as_deref() else {
                continue;
            };
            let mut chars = key.chars();
            if let (Some(c), None) = (chars.next(), chars.next())
                && !jumps.contains_key(&c)
            {
                jumps.insert(c, session.name.clone());
            }
        }
        jumps
    }

    fn validate(&self) -> Result<(), ConfigError> {
        let mut seen = HashSet::new();
        for session in &self.sessions {
            if session.name.is_empty() {
                return Err(ConfigError::Invalid("session name is empty".into()));
            }
            if session.argv.is_empty() {
                return Err(ConfigError::Invalid(format!(
                    "session {} has empty argv",
                    session.name
                )));
            }
            if !seen.insert(session.name.as_str()) {
                return Err(ConfigError::Invalid(format!(
                    "duplicate session name {}",
                    session.name
                )));
            }
            if session.project.as_deref().is_some_and(str::is_empty) {
                return Err(ConfigError::Invalid(format!(
                    "session {} has empty project",
                    session.name
                )));
            }
        }
        if self.detach_chord.is_some() {
            self.detach_chord()?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_named_sessions_including_optional_key() {
        let config = toml::from_str::<Config>(
            r#"
[[sessions]]
name = "kiro"
argv = ["kiro-cli"]
cwd = "/tmp/kiro"
key = "k"
project = "habitat"

[[sessions]]
name = "claude"
argv = ["claude"]
"#,
        )
        .unwrap();
        config.validate().unwrap();
        let kiro = config.lookup("kiro").unwrap();
        assert_eq!(kiro.argv, ["kiro-cli"]);
        assert_eq!(kiro.cwd.as_deref(), Some("/tmp/kiro"));
        assert_eq!(kiro.key.as_deref(), Some("k"));
        assert_eq!(kiro.project.as_deref(), Some("habitat"));
        assert!(config.lookup("missing").is_none());
    }

    #[test]
    fn rejects_duplicate_names() {
        let config = toml::from_str::<Config>(
            r#"
[[sessions]]
name = "kiro"
argv = ["kiro-cli"]
[[sessions]]
name = "kiro"
argv = ["cat"]
"#,
        )
        .unwrap();
        let err = config.validate().unwrap_err();
        assert!(err.to_string().contains("duplicate session name kiro"));
    }

    #[test]
    fn default_path_prefers_xdg_config_home() {
        let path =
            default_config_path_from(Some("/xdg-config".into()), Some("/home/operator".into()));
        assert_eq!(path, PathBuf::from("/xdg-config/sumika/config.toml"));
    }

    #[test]
    fn default_path_uses_home_dot_config() {
        let path = default_config_path_from(None, Some("/home/operator".into()));
        assert_eq!(
            path,
            PathBuf::from("/home/operator/.config/sumika/config.toml")
        );
    }

    #[test]
    fn explicit_path_wins() {
        let path = resolve_config_path(Some(PathBuf::from("/tmp/custom.toml")));
        assert_eq!(path, PathBuf::from("/tmp/custom.toml"));
    }
}
