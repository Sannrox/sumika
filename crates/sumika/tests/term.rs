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
    async fn start_with_term(term: Option<&str>) -> Self {
        let dir = TempDir::new().expect("tempdir");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o700))
                .expect("private tempdir");
        }
        let sock = dir.path().join("sumika.sock");
        let bin = PathBuf::from(env!("CARGO_BIN_EXE_sumika"));
        let mut cmd = Command::new(&bin);
        cmd.args(["daemon"])
            .env("SUMIKA_SOCK", &sock)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        match term {
            Some(value) => {
                cmd.env("TERM", value);
            }
            None => {
                cmd.env_remove("TERM");
            }
        }
        let daemon = cmd.spawn().expect("spawn daemon");
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

async fn start_term_child(client: &Client, name: &str) {
    let start = client
        .rpc(&Request::Start {
            name: name.into(),
            argv: vec![
                "sh".into(),
                "-c".into(),
                "printf 'TERM=%s\\n' \"$TERM\"; sleep 60".into(),
            ],
            cwd: None,
        })
        .await
        .expect("start");
    assert!(start.ok, "{start:?}");
}

async fn attach_sees_term(client: &Client, name: &str) -> String {
    let (resp, mut stream) = client.attach(name).await.expect("attach");
    assert!(resp.ok, "{resp:?}");
    let mut buf = vec![0u8; 8192];
    let deadline = tokio::time::Instant::now() + Duration::from_secs(2);
    let mut seen = String::new();
    while tokio::time::Instant::now() < deadline {
        let n = timeout(Duration::from_millis(400), stream.read(&mut buf))
            .await
            .ok()
            .and_then(Result::ok)
            .unwrap_or(0);
        if n > 0 {
            seen.push_str(&String::from_utf8_lossy(&buf[..n]));
        }
        if seen.contains("TERM=xterm-256color") {
            return seen;
        }
    }
    seen
}

#[tokio::test]
async fn child_gets_xterm_term_when_daemon_has_none() {
    let harness = Harness::start_with_term(None).await;
    start_term_child(&harness.client, "termcheck").await;
    let seen = attach_sees_term(&harness.client, "termcheck").await;
    assert!(
        seen.contains("TERM=xterm-256color"),
        "child TERM missing xterm-256color: {seen:?}"
    );
}

#[tokio::test]
async fn child_gets_xterm_term_even_if_daemon_term_is_dumb() {
    let harness = Harness::start_with_term(Some("dumb")).await;
    start_term_child(&harness.client, "termcheck").await;
    let seen = attach_sees_term(&harness.client, "termcheck").await;
    assert!(
        seen.contains("TERM=xterm-256color"),
        "child inherited daemon TERM: {seen:?}"
    );
    assert!(
        !seen.contains("TERM=dumb"),
        "child kept dumb TERM: {seen:?}"
    );
}
