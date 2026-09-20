use std::io::Write;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::time::Duration;

use sumika_ctl::Client;
use sumika_protocol::{Request, Status};
use tempfile::TempDir;
use tokio::io::AsyncReadExt;

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
}

impl Drop for Harness {
    fn drop(&mut self) {
        let _ = self.daemon.kill();
        let _ = self.daemon.wait();
    }
}

#[tokio::test]
async fn copy_mode_yanks_a_known_line_without_killing() {
    let harness = Harness::start().await;
    let clip = harness.dir.path().join("clip");
    let marker = "UNIQUE-COPY-LINE";
    let start = harness
        .client
        .rpc(&Request::Start {
            name: "demo".into(),
            argv: vec![
                "python3".into(),
                "-c".into(),
                format!(
                    "print({marker:?}, flush=True)\nfor i in range(40):\n    print(f'pad-{{i}}', flush=True)\nimport time; time.sleep(60)"
                ),
            ],
            cwd: None,
        })
        .await
        .expect("start");
    assert!(start.ok, "{start:?}");
    tokio::time::sleep(Duration::from_millis(400)).await;

    let lines = harness
        .client
        .rpc(&Request::Scrollback {
            name: "demo".into(),
        })
        .await
        .expect("scrollback")
        .scrollback
        .unwrap_or_default();
    assert!(
        lines.iter().any(|line| line.contains(marker)),
        "ring missing {marker}: {lines:?}"
    );

    let mut attach = Command::new(&harness.bin)
        .args(["attach", "demo"])
        .env("SUMIKA_SOCK", harness.client.sock_path())
        .env("SUMIKA_CLIPBOARD_FILE", &clip)
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
        stdin.write_all(&[0x1c, b'y']).unwrap();
        stdin.write_all(b"g").unwrap();
        stdin.write_all(b"y").unwrap();
        stdin.write_all(b"q").unwrap();
        stdin.write_all(&[0x1c, 0x02]).unwrap();
        stdin.flush().unwrap();
    }
    let output = attach.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "attach after copy/detach: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let deadline = tokio::time::Instant::now() + Duration::from_secs(2);
    loop {
        let text = std::fs::read_to_string(&clip).unwrap_or_default();
        if text.contains(marker) {
            break;
        }
        if tokio::time::Instant::now() > deadline {
            panic!("clipboard missing {marker}: {text:?}");
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
    assert_eq!(demo.status, Status::Running);

    let (second, _stream) = harness.client.attach("demo").await.unwrap();
    assert!(second.ok);
}

#[tokio::test]
async fn copy_mode_does_not_change_steal() {
    let harness = Harness::start().await;
    harness
        .client
        .rpc(&Request::Start {
            name: "demo".into(),
            argv: vec!["cat".into()],
            cwd: None,
        })
        .await
        .unwrap();
    let (first_resp, mut first) = harness.client.attach("demo").await.unwrap();
    assert!(first_resp.ok);
    let (second_resp, _second) = harness.client.attach("demo").await.unwrap();
    assert!(second_resp.ok);
    let n = tokio::time::timeout(Duration::from_secs(2), first.read(&mut [0u8; 8]))
        .await
        .expect("eof")
        .expect("read");
    assert_eq!(n, 0);
}
