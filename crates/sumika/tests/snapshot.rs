use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::time::Duration;

use sumika_ctl::Client;
use sumika_protocol::Request;
use tempfile::TempDir;
use tokio::io::AsyncReadExt;
use tokio::time::timeout;

struct Harness {
    daemon: Child,
    client: Client,
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
            _dir: dir,
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
async fn attach_replays_last_frame_before_new_input() {
    let harness = Harness::start().await;
    let marker = "UNIQUE-SNAPSHOT-XYZ";
    let start = harness
        .client
        .rpc(&Request::Start {
            name: "demo".into(),
            argv: vec![
                "python3".into(),
                "-c".into(),
                format!("print({marker:?}, flush=True); import time; time.sleep(60)"),
            ],
            cwd: None,
        })
        .await
        .expect("start");
    assert!(start.ok, "{start:?}");
    tokio::time::sleep(Duration::from_millis(200)).await;

    let (resp, mut stream) = harness.client.attach("demo").await.expect("attach");
    assert!(resp.ok, "{resp:?}");
    let mut buf = vec![0u8; 8192];
    let n = timeout(Duration::from_secs(2), stream.read(&mut buf))
        .await
        .expect("read timeout")
        .expect("read");
    let seen = String::from_utf8_lossy(&buf[..n]);
    assert!(
        seen.contains(marker),
        "last frame missing {marker}: {seen:?}"
    );
}
