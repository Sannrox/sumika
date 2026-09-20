use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::time::Duration;

use serde_json::Value;
use sumika_ctl::Client;
use sumika_protocol::Request;
use tempfile::TempDir;

struct Harness {
    daemon: Child,
    client: Client,
    bin: PathBuf,
    _dir: TempDir,
}

impl Harness {
    async fn start() -> Self {
        let dir = TempDir::new().expect("tempdir");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o700))
                .expect("private tempdir");
        }
        let sock = dir.path().join("sumika.sock");
        let bin = PathBuf::from(env!("CARGO_BIN_EXE_sumika"));
        let daemon = Command::new(&bin)
            .args(["daemon"])
            .env("SUMIKA_SOCK", &sock)
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
            _dir: dir,
        }
    }

    fn cmd(&self) -> Command {
        let mut cmd = Command::new(&self.bin);
        cmd.env("SUMIKA_SOCK", self.client.sock_path())
            .stdin(Stdio::null());
        cmd
    }
}

impl Drop for Harness {
    fn drop(&mut self) {
        let _ = self.daemon.kill();
        let _ = self.daemon.wait();
    }
}

#[tokio::test]
async fn doctor_json_is_nonzero_when_daemon_is_down() {
    let bin = PathBuf::from(env!("CARGO_BIN_EXE_sumika"));
    let dir = TempDir::new().unwrap();
    let sock = dir.path().join("missing.sock");
    let output = Command::new(&bin)
        .args(["doctor", "--json"])
        .env("SUMIKA_SOCK", &sock)
        .output()
        .unwrap();
    assert!(!output.status.success());
    let report: Value = serde_json::from_slice(&output.stdout).expect("doctor json");
    assert_eq!(report["reachable"], false);
    assert!(report["socket"].as_str().unwrap().ends_with("missing.sock"));
    assert!(report.get("launchd_loaded").is_some());
    assert!(report.get("systemd_user_loaded").is_some());
    assert!(report.get("config").is_some());
    assert_eq!(report["sessions"], serde_json::json!([]));
}

#[tokio::test]
async fn doctor_json_reports_reachable_sessions() {
    let harness = Harness::start().await;
    assert!(
        harness
            .cmd()
            .args(["start", "demo", "--", "cat"])
            .output()
            .unwrap()
            .status
            .success()
    );
    let output = harness.cmd().args(["doctor", "--json"]).output().unwrap();
    assert!(output.status.success());
    let report: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["reachable"], true);
    assert_eq!(report["sessions"][0]["name"], "demo");
    assert!(report["sessions"][0]["pid"].is_number());
}
