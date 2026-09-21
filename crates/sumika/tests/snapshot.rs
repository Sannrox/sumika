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

#[tokio::test]
async fn attach_replays_lines_that_left_the_viewport() {
    let harness = Harness::start().await;
    let start = harness
        .client
        .rpc(&Request::Start {
            name: "demo".into(),
            argv: vec![
                "python3".into(),
                "-c".into(),
                "import time\nprint('SCROLL-EARLY-UNIQUE', flush=True)\nfor i in range(40):\n    print(f'SCROLL-{i:03}-LINE', flush=True)\nprint('SCROLL-LATE-UNIQUE', flush=True)\ntime.sleep(60)".into(),
            ],
            cwd: None,
        })
        .await
        .expect("start");
    assert!(start.ok, "{start:?}");
    tokio::time::sleep(Duration::from_millis(400)).await;

    let (resp, mut stream) = harness.client.attach("demo").await.expect("attach");
    assert!(resp.ok, "{resp:?}");
    let seen = read_until_contains(&mut stream, "SCROLL-LATE-UNIQUE").await;
    assert!(
        seen.contains("SCROLL-EARLY-UNIQUE"),
        "earlier line missing from attach replay: {seen:?}"
    );
}

#[tokio::test]
async fn reattach_keeps_sgr_and_wide_column_from_last_frame() {
    let harness = Harness::start().await;
    let start = harness
        .client
        .rpc(&Request::Start {
            name: "demo".into(),
            argv: vec![
                "python3".into(),
                "-c".into(),
                "import sys, time\ntime.sleep(0.4)\nsys.stdout.write('\\033[31mRED-MARKER\\033[0m')\nsys.stdout.write('\\033[2;100HCOL100-MARKER')\nsys.stdout.flush()\ntime.sleep(60)".into(),
            ],
            cwd: None,
        })
        .await
        .expect("start");
    assert!(start.ok, "{start:?}");
    let resized = harness
        .client
        .rpc(&Request::Resize {
            name: "demo".into(),
            cols: 120,
            rows: 24,
        })
        .await
        .expect("resize");
    assert!(resized.ok, "{resized:?}");
    tokio::time::sleep(Duration::from_millis(700)).await;

    {
        let (resp, _stream) = harness.client.attach("demo").await.expect("first attach");
        assert!(resp.ok, "{resp:?}");
    }
    tokio::time::sleep(Duration::from_millis(50)).await;

    let (resp, mut stream) = harness.client.attach("demo").await.expect("reattach");
    assert!(resp.ok, "{resp:?}");
    let mut buf = vec![0u8; 16 * 1024];
    let n = timeout(Duration::from_secs(2), stream.read(&mut buf))
        .await
        .expect("read timeout")
        .expect("read");
    let seen = String::from_utf8_lossy(&buf[..n]);
    assert!(
        seen.contains("RED-MARKER"),
        "red marker missing from last frame: {seen:?}"
    );
    assert!(
        seen.contains("\u{1b}[31m") || seen.contains("\u{1b}[0;31m"),
        "SGR red missing from last frame: {seen:?}"
    );
    assert!(
        seen.contains("COL100-MARKER"),
        "wide column marker missing from last frame: {seen:?}"
    );
    let start = seen.find("COL100-MARKER").expect("marker");
    let row = seen[..start]
        .rsplit("\r\n")
        .next()
        .unwrap_or(&seen[..start]);
    let visible = strip_csi(row);
    assert!(
        visible.chars().count() >= 99,
        "COL100-MARKER wrapped into 80 columns: {visible:?}"
    );
}

fn strip_csi(text: &str) -> String {
    let mut out = String::new();
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\u{1b}' {
            if chars.peek() == Some(&'[') {
                chars.next();
                for next in chars.by_ref() {
                    if next.is_ascii_alphabetic() {
                        break;
                    }
                }
                continue;
            }
            continue;
        }
        out.push(c);
    }
    out
}

#[tokio::test]
async fn stalled_client_does_not_block_the_child() {
    let harness = Harness::start().await;
    let start = harness
        .client
        .rpc(&Request::Start {
            name: "demo".into(),
            argv: vec![
                "python3".into(),
                "-c".into(),
                "import time\nprint('STALL-A-UNIQUE', flush=True)\ntime.sleep(0.2)\nfor i in range(200):\n    print(f'pad-{i}', flush=True)\nprint('STALL-B-UNIQUE', flush=True)\ntime.sleep(60)".into(),
            ],
            cwd: None,
        })
        .await
        .expect("start");
    assert!(start.ok, "{start:?}");

    let (first_resp, _first) = harness.client.attach("demo").await.expect("first attach");
    assert!(first_resp.ok, "{first_resp:?}");
    tokio::time::sleep(Duration::from_millis(600)).await;

    let (resp, mut stream) = harness.client.attach("demo").await.expect("steal");
    assert!(resp.ok, "{resp:?}");
    let seen = read_until_contains(&mut stream, "STALL-B-UNIQUE").await;
    assert!(
        seen.contains("STALL-B-UNIQUE"),
        "child stalled behind unread attach: {seen:?}"
    );
}

async fn read_until_contains(stream: &mut (impl AsyncReadExt + Unpin), token: &str) -> String {
    let mut buf = Vec::new();
    let mut chunk = [0u8; 4096];
    let deadline = tokio::time::Instant::now() + Duration::from_secs(3);
    loop {
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        if remaining.is_zero() {
            let seen = String::from_utf8_lossy(&buf).into_owned();
            panic!("did not see {token:?} in {seen:?}");
        }
        let n = timeout(remaining, stream.read(&mut chunk))
            .await
            .unwrap_or_else(|_| panic!("read timeout before {token:?}"))
            .expect("read");
        if n == 0 {
            let seen = String::from_utf8_lossy(&buf).into_owned();
            panic!("EOF before {token:?} in {seen:?}");
        }
        buf.extend_from_slice(&chunk[..n]);
        let seen = String::from_utf8_lossy(&buf).into_owned();
        if seen.contains(token) {
            return seen;
        }
    }
}
