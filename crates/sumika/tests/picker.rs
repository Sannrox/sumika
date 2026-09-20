use std::collections::HashMap;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::time::Duration;

use sumika::picker::{Action, Input, Picker, attention_line};
use sumika_ctl::Client;
use sumika_protocol::{Request, Status};
use tempfile::TempDir;
use tokio::io::AsyncWriteExt;

struct Harness {
    daemon: Child,
    client: Client,
    bin: PathBuf,
    config: PathBuf,
    kiro_cwd: PathBuf,
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
        let kiro_cwd = dir.path().join("kiro-home");
        std::fs::create_dir(&kiro_cwd).expect("kiro cwd");
        let config = dir.path().join("config.toml");
        std::fs::write(
            &config,
            format!(
                r#"
[[sessions]]
name = "claude"
argv = ["cat"]
key = "c"

[[sessions]]
name = "grok"
argv = ["cat"]
key = "g"

[[sessions]]
name = "kiro"
argv = ["cat"]
cwd = "{}"
key = "k"

[[sessions]]
name = "pi"
argv = ["cat"]
key = "p"

[[sessions]]
name = "codex"
argv = ["cat"]
key = "x"
"#,
                kiro_cwd.display()
            ),
        )
        .expect("write config");
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
            config,
            kiro_cwd,
            _dir: dir,
        }
    }

    fn cmd(&self) -> Command {
        let mut cmd = Command::new(&self.bin);
        cmd.env("SUMIKA_SOCK", self.client.sock_path())
            .env("SUMIKA_CONFIG", &self.config)
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

async fn wait_status(client: &Client, name: &str, status: Status) -> sumika_protocol::SessionInfo {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(2);
    loop {
        let listed = client.rpc(&Request::List).await.expect("list");
        let session = listed
            .sessions
            .unwrap()
            .into_iter()
            .find(|session| session.name == name)
            .expect("session");
        if session.status == status {
            return session;
        }
        if tokio::time::Instant::now() > deadline {
            panic!("{name} stayed {:?}", session.status);
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

#[tokio::test]
async fn picker_lists_five_sessions_enter_attaches_q_leaves_children() {
    let harness = Harness::start().await;
    assert!(
        harness
            .cmd()
            .args(["start", "--all"])
            .output()
            .unwrap()
            .status
            .success()
    );
    let sessions = harness
        .client
        .rpc(&Request::List)
        .await
        .unwrap()
        .sessions
        .unwrap();
    assert_eq!(
        sessions
            .iter()
            .map(|session| session.name.as_str())
            .collect::<Vec<_>>(),
        ["claude", "codex", "grok", "kiro", "pi"]
    );

    let mut picker = Picker::new(sessions, HashMap::new());
    assert_eq!(picker.handle(Input::Quit), Action::Quit);
    assert_eq!(
        picker.handle(Input::Attach),
        Action::Attach("claude".into())
    );
    let (resp, mut stream) = harness.client.attach("claude").await.unwrap();
    assert!(resp.ok, "{resp:?}");
    stream.write_all(b"hello\n").await.unwrap();
    drop(stream);

    let running = harness
        .client
        .rpc(&Request::List)
        .await
        .unwrap()
        .sessions
        .unwrap()
        .into_iter()
        .filter(|session| session.status == Status::Running)
        .count();
    assert_eq!(running, 5);
}

#[tokio::test]
async fn picker_r_starts_dead_kiro_from_config() {
    let harness = Harness::start().await;
    assert!(
        harness
            .cmd()
            .args(["start", "kiro", "--", "true"])
            .output()
            .unwrap()
            .status
            .success()
    );
    wait_status(&harness.client, "kiro", Status::Dead).await;

    let sessions = harness
        .client
        .rpc(&Request::List)
        .await
        .unwrap()
        .sessions
        .unwrap();
    let mut picker = Picker::new(sessions, HashMap::new());
    assert_eq!(
        picker.handle(Input::Restart),
        Action::Restart("kiro".into())
    );

    assert!(
        harness
            .cmd()
            .args(["start", "kiro"])
            .output()
            .unwrap()
            .status
            .success()
    );
    let kiro = wait_status(&harness.client, "kiro", Status::Running).await;
    assert_eq!(kiro.argv, ["cat"]);
    assert_eq!(std::path::Path::new(&kiro.cwd), harness.kiro_cwd.as_path());
}

#[tokio::test]
async fn report_blocked_sorts_to_top_of_picker_without_desktop_notify() {
    let harness = Harness::start().await;
    assert!(
        harness
            .cmd()
            .args(["start", "claude", "--", "cat"])
            .output()
            .unwrap()
            .status
            .success()
    );
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
            .args(["report", "kiro", "blocked"])
            .output()
            .unwrap()
            .status
            .success()
    );
    let sessions = harness
        .client
        .rpc(&Request::List)
        .await
        .unwrap()
        .sessions
        .unwrap();
    let picker = Picker::new(sessions, HashMap::new());
    assert_eq!(picker.rows()[0].name, "kiro");
    assert_eq!(picker.rows()[0].status, Status::Blocked);
    assert!(
        attention_line(picker.rows()).starts_with("kiro !"),
        "{}",
        attention_line(picker.rows())
    );
}
