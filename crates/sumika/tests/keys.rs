use std::os::unix::fs::PermissionsExt;
use std::process::{Command, Stdio};

use tempfile::TempDir;

#[test]
fn keys_prints_shortcuts_and_configured_jumps() {
    let dir = TempDir::new().unwrap();
    std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
    let config = dir.path().join("config.toml");
    std::fs::write(
        &config,
        r#"
[[sessions]]
name = "kiro"
argv = ["cat"]
key = "k"
"#,
    )
    .unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_sumika"))
        .args(["keys"])
        .env("SUMIKA_CONFIG", &config)
        .stdin(Stdio::null())
        .output()
        .unwrap();
    assert!(out.status.success());
    let text = String::from_utf8(out.stdout).unwrap();
    assert!(text.contains("j/k"), "{text}");
    assert!(text.contains("enter"), "{text}");
    assert!(text.contains("spc k   jump kiro"), "{text}");
    assert!(text.contains("C-b then q"), "{text}");
    assert!(text.contains("?       toggle this help"), "{text}");
}
