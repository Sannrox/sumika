use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::time::Duration;

use sumika_ctl::Client;
use sumika_protocol::{ErrorCode, Request, Status};
use tempfile::TempDir;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
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
        let bin = env!("CARGO_BIN_EXE_sumika");
        let daemon = Command::new(bin)
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
async fn cat_survives_detach_and_reattach() {
    let harness = Harness::start().await;
    let start = harness
        .client
        .rpc(&Request::Start {
            name: "demo".into(),
            argv: vec!["cat".into()],
            cwd: None,
            project: None,
        })
        .await
        .expect("start");
    assert!(start.ok, "{start:?}");
    let pid = start.session.as_ref().and_then(|s| s.pid).expect("pid");

    {
        let (resp, mut stream) = harness.client.attach("demo").await.expect("attach");
        assert!(resp.ok, "{resp:?}");
        stream.write_all(b"hello\n").await.unwrap();
        read_until(&mut stream, "hello").await;
    }

    let demo = wait_unfocused(&harness.client, "demo").await;
    assert_eq!(demo.pid, Some(pid));
    assert_eq!(demo.status, Status::Running);

    let (resp, mut stream) = harness.client.attach("demo").await.expect("reattach");
    assert!(resp.ok, "{resp:?}");
    stream.write_all(b"world\n").await.unwrap();
    read_until(&mut stream, "world").await;
    assert_eq!(
        harness
            .client
            .rpc(&Request::List)
            .await
            .unwrap()
            .sessions
            .unwrap()
            .iter()
            .find(|s| s.name == "demo")
            .unwrap()
            .pid,
        Some(pid)
    );
}

#[tokio::test]
async fn second_attach_steals_the_first() {
    let harness = Harness::start().await;
    harness
        .client
        .rpc(&Request::Start {
            name: "demo".into(),
            argv: vec!["cat".into()],
            cwd: None,
            project: None,
        })
        .await
        .unwrap();

    let (first_resp, mut first) = harness.client.attach("demo").await.unwrap();
    assert!(first_resp.ok);
    let (second_resp, mut second) = harness.client.attach("demo").await.unwrap();
    assert!(second_resp.ok);

    let n = timeout(Duration::from_secs(2), first.read(&mut [0u8; 8]))
        .await
        .expect("first attach should EOF after steal")
        .expect("read");
    assert_eq!(n, 0);

    second.write_all(b"stolen\n").await.unwrap();
    read_until(&mut second, "stolen").await;
}

#[tokio::test]
async fn resize_reaches_the_child() {
    let harness = Harness::start().await;
    let script = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/winsize.py");
    harness
        .client
        .rpc(&Request::Start {
            name: "size".into(),
            argv: vec!["python3".into(), script.display().to_string()],
            cwd: None,
            project: None,
        })
        .await
        .unwrap();

    let (resp, mut stream) = harness.client.attach("size").await.unwrap();
    assert!(resp.ok, "{resp:?}");

    let first = harness
        .client
        .rpc(&Request::Resize {
            name: "size".into(),
            cols: 90,
            rows: 30,
        })
        .await
        .unwrap();
    assert!(first.ok, "{first:?}");
    read_until(&mut stream, "30x90").await;

    let second = harness
        .client
        .rpc(&Request::Resize {
            name: "size".into(),
            cols: 40,
            rows: 12,
        })
        .await
        .unwrap();
    assert!(second.ok, "{second:?}");
    read_until(&mut stream, "12x40").await;
}

#[tokio::test]
async fn start_replaces_a_dead_session() {
    let harness = Harness::start().await;
    let first = harness
        .client
        .rpc(&Request::Start {
            name: "demo".into(),
            argv: vec!["true".into()],
            cwd: None,
            project: None,
        })
        .await
        .unwrap();
    assert!(first.ok, "{first:?}");
    let first_pid = first.session.as_ref().and_then(|session| session.pid);
    let deadline = tokio::time::Instant::now() + Duration::from_secs(2);
    loop {
        let listed = harness.client.rpc(&Request::List).await.unwrap();
        let demo = listed
            .sessions
            .unwrap()
            .into_iter()
            .find(|session| session.name == "demo")
            .expect("demo");
        if demo.status == Status::Dead {
            break;
        }
        if tokio::time::Instant::now() > deadline {
            panic!("session stayed {:?}", demo.status);
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    let second = harness
        .client
        .rpc(&Request::Start {
            name: "demo".into(),
            argv: vec!["cat".into()],
            cwd: None,
            project: None,
        })
        .await
        .unwrap();
    assert!(second.ok, "{second:?}");
    let session = second.session.unwrap();
    assert_eq!(session.status, Status::Running);
    assert_ne!(session.pid, first_pid);
    assert_eq!(session.argv, vec!["cat"]);
}

#[tokio::test]
async fn start_rejects_missing_cwd() {
    let harness = Harness::start().await;
    let response = harness
        .client
        .rpc(&Request::Start {
            name: "demo".into(),
            argv: vec!["cat".into()],
            cwd: Some("/no/such/sumika-cwd".into()),
            project: None,
        })
        .await
        .unwrap();
    assert!(!response.ok, "{response:?}");
    assert_eq!(
        response.error.as_ref().map(|error| error.code.clone()),
        Some(ErrorCode::SpawnFailed)
    );
}

#[tokio::test]
async fn start_cli_resolves_relative_cwd() {
    let harness = Harness::start().await;
    let here = std::env::current_dir().unwrap();
    let status = Command::new(env!("CARGO_BIN_EXE_sumika"))
        .args(["start", "demo", "--cwd", ".", "--", "cat"])
        .env("SUMIKA_SOCK", harness.client.sock_path())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .unwrap();
    assert!(status.success());
    let listed = harness.client.rpc(&Request::List).await.unwrap();
    let demo = listed
        .sessions
        .unwrap()
        .into_iter()
        .find(|s| s.name == "demo")
        .expect("demo");
    assert!(
        std::path::Path::new(&demo.cwd).is_absolute(),
        "cwd should be absolute, got {}",
        demo.cwd
    );
    assert_eq!(std::path::Path::new(&demo.cwd), here.as_path());
}

#[tokio::test]
async fn child_exit_marks_session_dead_without_zombie() {
    let harness = Harness::start().await;
    let start = harness
        .client
        .rpc(&Request::Start {
            name: "demo".into(),
            argv: vec!["true".into()],
            cwd: None,
            project: None,
        })
        .await
        .unwrap();
    assert!(start.ok, "{start:?}");
    let pid = start.session.as_ref().and_then(|s| s.pid);

    let deadline = tokio::time::Instant::now() + Duration::from_secs(2);
    loop {
        let listed = harness.client.rpc(&Request::List).await.unwrap();
        let demo = listed
            .sessions
            .unwrap()
            .into_iter()
            .find(|s| s.name == "demo")
            .expect("demo");
        if demo.status == Status::Dead {
            if let Some(pid) = pid {
                assert!(!pid_exists(pid), "pid {pid} still exists after child exit");
            }
            return;
        }
        if tokio::time::Instant::now() > deadline {
            panic!("session stayed {:?} after child exited", demo.status);
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

#[tokio::test]
async fn kill_reaps_the_child() {
    let harness = Harness::start().await;
    let start = harness
        .client
        .rpc(&Request::Start {
            name: "demo".into(),
            argv: vec!["cat".into()],
            cwd: None,
            project: None,
        })
        .await
        .unwrap();
    let pid = start.session.unwrap().pid.unwrap();

    let killed = harness
        .client
        .rpc(&Request::Kill {
            name: "demo".into(),
            force: true,
        })
        .await
        .unwrap();
    assert!(killed.ok, "{killed:?}");

    let deadline = tokio::time::Instant::now() + Duration::from_secs(2);
    loop {
        if !pid_exists(pid) {
            break;
        }
        if tokio::time::Instant::now() > deadline {
            panic!("pid {pid} still exists after kill (zombie or leak)");
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

async fn wait_unfocused(client: &Client, name: &str) -> sumika_protocol::SessionInfo {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(2);
    loop {
        let listed = client.rpc(&Request::List).await.expect("list");
        let demo = listed
            .sessions
            .unwrap()
            .into_iter()
            .find(|session| session.name == name)
            .expect("session");
        if !demo.focused {
            return demo;
        }
        if tokio::time::Instant::now() > deadline {
            panic!("session {name} stayed focused after detach");
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

fn normalize(text: &str) -> String {
    text.replace('\r', "")
}

async fn read_until(stream: &mut tokio::net::UnixStream, token: &str) {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(3);
    loop {
        let line = read_line(stream).await;
        if normalize(&line).contains(token) {
            return;
        }
        if tokio::time::Instant::now() > deadline {
            panic!("did not see {token:?} in child output");
        }
    }
}

async fn read_line(stream: &mut tokio::net::UnixStream) -> String {
    let mut buf = Vec::new();
    let mut byte = [0u8; 1];
    timeout(Duration::from_secs(3), async {
        loop {
            stream.read_exact(&mut byte).await.unwrap();
            buf.push(byte[0]);
            if byte[0] == b'\n' {
                break;
            }
        }
    })
    .await
    .expect("timed out waiting for child output");
    String::from_utf8(buf).unwrap()
}

fn pid_exists(pid: u32) -> bool {
    let rc = unsafe { libc::kill(pid as libc::pid_t, 0) };
    if rc == 0 {
        return true;
    }
    let err = std::io::Error::last_os_error().raw_os_error().unwrap_or(0);
    err == libc::EPERM
}
