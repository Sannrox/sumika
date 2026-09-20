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
    notify: PathBuf,
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
        let notify = dir.path().join("notify.log");
        let bin = PathBuf::from(env!("CARGO_BIN_EXE_sumika"));
        let daemon = Command::new(&bin)
            .args(["daemon"])
            .env("SUMIKA_SOCK", &sock)
            .env("SUMIKA_NOTIFY_FILE", &notify)
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
            notify,
            _dir: dir,
        }
    }

    fn cmd(&self) -> Command {
        let mut cmd = Command::new(&self.bin);
        cmd.env("SUMIKA_SOCK", self.client.sock_path())
            .env("SUMIKA_NOTIFY_FILE", &self.notify)
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
async fn report_idle_on_unfocused_session_notifies() {
    let harness = Harness::start().await;
    assert!(
        harness
            .cmd()
            .args(["start", "kiro", "--", "cat"])
            .output()
            .unwrap()
            .status
            .success()
    );
    assert!(
        harness
            .cmd()
            .args(["report", "kiro", "idle"])
            .output()
            .unwrap()
            .status
            .success()
    );
    let listed = harness.client.rpc(&Request::List).await.unwrap();
    let kiro = listed
        .sessions
        .unwrap()
        .into_iter()
        .find(|session| session.name == "kiro")
        .unwrap();
    assert_eq!(kiro.status, Status::Idle);
    assert!(!kiro.focused);
    let notify = std::fs::read_to_string(&harness.notify).unwrap();
    assert_eq!(notify, "kiro\tidle\n");
    assert!(!notify.contains("PTY"));
}

#[tokio::test]
async fn report_on_focused_session_does_not_notify() {
    let harness = Harness::start().await;
    assert!(
        harness
            .cmd()
            .args(["start", "kiro", "--", "cat"])
            .output()
            .unwrap()
            .status
            .success()
    );
    let (resp, _stream) = harness.client.attach("kiro").await.unwrap();
    assert!(resp.ok);
    assert!(
        harness
            .cmd()
            .args(["report", "kiro", "blocked"])
            .output()
            .unwrap()
            .status
            .success()
    );
    let listed = harness.client.rpc(&Request::List).await.unwrap();
    let kiro = listed
        .sessions
        .unwrap()
        .into_iter()
        .find(|session| session.name == "kiro")
        .unwrap();
    assert_eq!(kiro.status, Status::Blocked);
    assert!(kiro.focused);
    assert!(!harness.notify.exists());
}
