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
        wait_for_ping(&client).await;
        Self {
            daemon,
            client,
            bin,
            dir,
        }
    }
}

impl Drop for Harness {
    fn drop(&mut self) {
        let _ = self.daemon.kill();
        let _ = self.daemon.wait();
    }
}

async fn wait_for_ping(client: &Client) {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    loop {
        if client.rpc(&Request::Ping).await.is_ok() {
            return;
        }
        if tokio::time::Instant::now() > deadline {
            panic!("daemon did not become reachable");
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

#[tokio::test]
async fn detach_chord_returns_without_killing_or_forwarding() {
    let harness = Harness::start().await;
    let recorded = harness.dir.path().join("recorded.bin");
    let script = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/record_bytes.py");
    let script_s = script.display().to_string();
    let recorded_s = recorded.display().to_string();
    let start = Command::new(&harness.bin)
        .args(["start", "demo", "--", "python3", &script_s, &recorded_s])
        .env("SUMIKA_SOCK", harness.client.sock_path())
        .output()
        .unwrap();
    assert!(
        start.status.success(),
        "{}",
        String::from_utf8_lossy(&start.stderr)
    );
    let pid = harness
        .client
        .rpc(&Request::List)
        .await
        .unwrap()
        .sessions
        .unwrap()
        .into_iter()
        .find(|session| session.name == "demo")
        .unwrap()
        .pid;

    let mut attach = Command::new(&harness.bin)
        .args(["attach", "demo"])
        .env("SUMIKA_SOCK", harness.client.sock_path())
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let focused = tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            let listed = harness.client.rpc(&Request::List).await.unwrap();
            if listed
                .sessions
                .unwrap()
                .iter()
                .any(|session| session.name == "demo" && session.focused)
            {
                return;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await;
    assert!(focused.is_ok(), "attach did not focus");
    {
        let stdin = attach.stdin.as_mut().unwrap();
        use std::io::Write;
        stdin.write_all(b"hello\n").unwrap();
        stdin.write_all(&[0x1c, 0x02]).unwrap();
        stdin.flush().unwrap();
    }
    let output = attach.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "attach should exit 0 after detach chord: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let deadline = tokio::time::Instant::now() + Duration::from_secs(2);
    loop {
        if recorded.exists() && std::fs::read(&recorded).unwrap_or_default() == b"hello\n" {
            break;
        }
        if tokio::time::Instant::now() > deadline {
            panic!(
                "child saw {:?}, expected hello\\\\n without chord bytes",
                std::fs::read(&recorded).ok()
            );
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }

    let demo = harness
        .client
        .rpc(&Request::List)
        .await
        .unwrap()
        .sessions
        .unwrap()
        .into_iter()
        .find(|session| session.name == "demo")
        .unwrap();
    assert_eq!(demo.pid, pid);
    assert_eq!(demo.status, Status::Running);
}
