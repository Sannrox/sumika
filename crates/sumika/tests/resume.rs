use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::time::Duration;

use sumika_ctl::Client;
use sumika_protocol::{Request, Status};
use tempfile::TempDir;

struct Harness {
    daemon: Child,
    client: Client,
    bin: PathBuf,
    dir: TempDir,
}

impl Harness {
    async fn start() -> Self {
        let dir = TempDir::new().expect("tempdir");
        std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
        let sock = dir.path().join("sumika.sock");
        let state = dir.path().join("state");
        std::fs::create_dir_all(&state).unwrap();
        let bin = PathBuf::from(env!("CARGO_BIN_EXE_sumika"));
        let daemon = Command::new(&bin)
            .args(["daemon"])
            .env("SUMIKA_SOCK", &sock)
            .env("SUMIKA_STATE_DIR", &state)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("spawn daemon");
        let client = Client::new(&sock);
        let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
        loop {
            if client.rpc(&Request::Ping).await.is_ok() {
                break;
            }
            if tokio::time::Instant::now() > deadline {
                panic!("daemon did not become reachable");
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        Self {
            daemon,
            client,
            bin,
            dir,
        }
    }

    fn fake_bin(&self, name: &str) -> PathBuf {
        let src = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/record_argv.py");
        let dest = self.dir.path().join(name);
        std::fs::copy(&src, &dest).unwrap();
        std::fs::set_permissions(&dest, std::fs::Permissions::from_mode(0o755)).unwrap();
        dest
    }

    fn hook_report(&self, name: &str, payload: &str) {
        let mut child = Command::new(&self.bin)
            .args(["hook-report"])
            .env("SUMIKA_SOCK", self.client.sock_path())
            .env("SUMIKA_SESSION", name)
            .env("SUMIKA_STATE_DIR", self.dir.path().join("state"))
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        child
            .stdin
            .as_mut()
            .unwrap()
            .write_all(payload.as_bytes())
            .unwrap();
        let output = child.wait_with_output().unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    async fn wait_dead(&self, name: &str) {
        let deadline = tokio::time::Instant::now() + Duration::from_secs(2);
        loop {
            let listed = self.client.rpc(&Request::List).await.unwrap();
            let session = listed
                .sessions
                .unwrap()
                .into_iter()
                .find(|session| session.name == name)
                .expect("session");
            if session.status == Status::Dead {
                return;
            }
            if tokio::time::Instant::now() > deadline {
                panic!("session stayed {:?}", session.status);
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    }
}

impl Drop for Harness {
    fn drop(&mut self) {
        let _ = self.daemon.kill();
        let _ = self.daemon.wait();
    }
}

#[tokio::test]
async fn claude_stop_hint_is_passed_on_restart() {
    let harness = Harness::start().await;
    let argv_file = harness.dir.path().join("argv.json");
    let claude = harness.fake_bin("claude");
    let start = Command::new(&harness.bin)
        .args([
            "start",
            "demo",
            "--",
            claude.to_str().unwrap(),
            argv_file.to_str().unwrap(),
        ])
        .env("SUMIKA_SOCK", harness.client.sock_path())
        .env("SUMIKA_STATE_DIR", harness.dir.path().join("state"))
        .output()
        .unwrap();
    assert!(
        start.status.success(),
        "{}",
        String::from_utf8_lossy(&start.stderr)
    );
    harness.hook_report(
        "demo",
        r#"{"hook_event_name":"Stop","session_id":"abc-123"}"#,
    );
    harness
        .client
        .rpc(&Request::Kill {
            name: "demo".into(),
            force: true,
        })
        .await
        .unwrap();
    harness.wait_dead("demo").await;
    let _ = std::fs::remove_file(&argv_file);
    let restart = Command::new(&harness.bin)
        .args([
            "start",
            "demo",
            "--",
            claude.to_str().unwrap(),
            argv_file.to_str().unwrap(),
        ])
        .env("SUMIKA_SOCK", harness.client.sock_path())
        .env("SUMIKA_STATE_DIR", harness.dir.path().join("state"))
        .output()
        .unwrap();
    assert!(
        restart.status.success(),
        "{}",
        String::from_utf8_lossy(&restart.stderr)
    );
    let deadline = tokio::time::Instant::now() + Duration::from_secs(2);
    let seen = loop {
        let text = std::fs::read_to_string(&argv_file).unwrap_or_default();
        if text.contains("abc-123") {
            break text;
        }
        if tokio::time::Instant::now() > deadline {
            panic!("missing resume id in {text:?}");
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    };
    assert!(seen.contains("--resume"), "{seen}");
}

#[tokio::test]
async fn kiro_stop_does_not_add_argv() {
    let harness = Harness::start().await;
    let argv_file = harness.dir.path().join("argv.json");
    let kiro = harness.fake_bin("kiro-cli");
    let start = Command::new(&harness.bin)
        .args([
            "start",
            "kiro",
            "--",
            kiro.to_str().unwrap(),
            argv_file.to_str().unwrap(),
        ])
        .env("SUMIKA_SOCK", harness.client.sock_path())
        .env("SUMIKA_STATE_DIR", harness.dir.path().join("state"))
        .output()
        .unwrap();
    assert!(start.status.success());
    harness.hook_report(
        "kiro",
        r#"{"hook_event_name":"stop","session_id":"should-not-pass"}"#,
    );
    harness
        .client
        .rpc(&Request::Kill {
            name: "kiro".into(),
            force: true,
        })
        .await
        .unwrap();
    harness.wait_dead("kiro").await;
    let _ = std::fs::remove_file(&argv_file);
    let restart = Command::new(&harness.bin)
        .args([
            "start",
            "kiro",
            "--",
            kiro.to_str().unwrap(),
            argv_file.to_str().unwrap(),
        ])
        .env("SUMIKA_SOCK", harness.client.sock_path())
        .env("SUMIKA_STATE_DIR", harness.dir.path().join("state"))
        .output()
        .unwrap();
    assert!(restart.status.success());
    let deadline = tokio::time::Instant::now() + Duration::from_secs(2);
    let seen = loop {
        let text = std::fs::read_to_string(&argv_file).unwrap_or_default();
        if !text.is_empty() {
            break text;
        }
        if tokio::time::Instant::now() > deadline {
            panic!("argv file empty");
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    };
    assert!(
        !seen.contains("should-not-pass"),
        "kiro must not get a resume flag: {seen}"
    );
    assert!(!seen.contains("--resume"), "{seen}");
}
