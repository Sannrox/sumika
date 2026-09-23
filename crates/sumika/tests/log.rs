use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::time::Duration;

use sumika_ctl::Client;
use sumika_protocol::{Request, Status};
use tempfile::TempDir;

struct Harness {
    daemon: Child,
    client: Client,
    log: PathBuf,
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
        let log = dir.path().join("daemon.log");
        let bin = PathBuf::from(env!("CARGO_BIN_EXE_sumika"));
        let daemon = Command::new(&bin)
            .args(["daemon"])
            .env("SUMIKA_SOCK", &sock)
            .env("SUMIKA_LOG", &log)
            .env("SUMIKA_NOTIFY_FILE", dir.path().join("notify.log"))
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
            log,
            _dir: dir,
        }
    }

    fn read_log(&self) -> String {
        std::fs::read_to_string(&self.log).unwrap_or_default()
    }
}

impl Drop for Harness {
    fn drop(&mut self) {
        let _ = self.daemon.kill();
        let _ = self.daemon.wait();
    }
}

#[tokio::test]
async fn operator_log_records_lifecycle_without_pty_bytes() {
    let harness = Harness::start().await;
    let marker = "UNIQUE-PTY-LOG-XYZ";
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
            project: None,
        })
        .await
        .expect("start");
    assert!(start.ok, "{start:?}");
    tokio::time::sleep(Duration::from_millis(200)).await;

    let report = harness
        .client
        .rpc(&Request::Report {
            name: "demo".into(),
            status: Status::Idle,
        })
        .await
        .expect("report");
    assert!(report.ok, "{report:?}");

    let (first_resp, _first) = harness.client.attach("demo").await.expect("attach");
    assert!(first_resp.ok, "{first_resp:?}");
    let (second_resp, _second) = harness.client.attach("demo").await.expect("steal");
    assert!(second_resp.ok, "{second_resp:?}");

    let kill = harness
        .client
        .rpc(&Request::Kill {
            name: "demo".into(),
            force: true,
        })
        .await
        .expect("kill");
    assert!(kill.ok, "{kill:?}");
    let deadline = tokio::time::Instant::now() + Duration::from_secs(2);
    loop {
        let log = harness.read_log();
        if log.contains("reaped") {
            break;
        }
        if tokio::time::Instant::now() > deadline {
            panic!("did not see reaped in {log}");
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }

    let log = harness.read_log();
    assert!(log.contains("listening"), "missing listen in {log}");
    assert!(log.contains("started"), "missing start in {log}");
    assert!(log.contains("stolen"), "missing steal in {log}");
    assert!(log.contains("reaped"), "missing reap in {log}");
    assert!(log.contains("report"), "missing report in {log}");
    assert!(log.contains("notify"), "missing notify in {log}");
    assert!(
        !log.contains(marker),
        "PTY payload leaked into operator log: {log}"
    );
}
