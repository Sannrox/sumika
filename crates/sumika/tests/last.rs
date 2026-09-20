use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::time::Duration;

use sumika_ctl::Client;
use sumika_protocol::Request;
use tempfile::TempDir;

struct Harness {
    daemon: Child,
    client: Client,
    bin: PathBuf,
    state: PathBuf,
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
        let state = dir.path().join("state");
        std::fs::create_dir(&state).unwrap();
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
            state,
            _dir: dir,
        }
    }

    fn cmd(&self) -> Command {
        let mut cmd = Command::new(&self.bin);
        cmd.env("SUMIKA_SOCK", self.client.sock_path())
            .env("SUMIKA_STATE_DIR", &self.state)
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
async fn nameless_attach_uses_last_attached_name() {
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
    let mut attach = harness
        .cmd()
        .args(["attach", "kiro"])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let deadline = tokio::time::Instant::now() + Duration::from_secs(2);
    loop {
        let listed = harness.client.rpc(&Request::List).await.unwrap();
        if listed
            .sessions
            .unwrap()
            .iter()
            .any(|session| session.name == "kiro" && session.focused)
        {
            break;
        }
        if tokio::time::Instant::now() > deadline {
            panic!("attach did not focus");
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    drop(attach.stdin.take());
    let _ = attach.wait();

    let mut again = harness
        .cmd()
        .args(["attach"])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let deadline = tokio::time::Instant::now() + Duration::from_secs(2);
    loop {
        let listed = harness.client.rpc(&Request::List).await.unwrap();
        if listed
            .sessions
            .unwrap()
            .iter()
            .any(|session| session.name == "kiro" && session.focused)
        {
            break;
        }
        if tokio::time::Instant::now() > deadline {
            panic!("nameless attach did not focus kiro");
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    drop(again.stdin.take());
    let _ = again.wait();
}

#[tokio::test]
async fn nameless_attach_without_state_is_nonzero() {
    let harness = Harness::start().await;
    let output = harness.cmd().args(["attach"]).output().unwrap();
    assert!(!output.status.success());
    let err = String::from_utf8_lossy(&output.stderr);
    assert!(
        err.contains("no last-attached session"),
        "unexpected stderr: {err}"
    );
}
