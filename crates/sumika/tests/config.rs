use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::time::Duration;

use sumika_ctl::Client;
use sumika_protocol::{Request, Response, SessionInfo};
use tempfile::TempDir;

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
project = "habitat"

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

    fn run(&self, args: &[&str]) -> std::process::Output {
        self.cmd().args(args).output().expect("sumika")
    }

    fn list_sessions(&self) -> Vec<SessionInfo> {
        let output = self.run(&["list", "--json"]);
        assert!(
            output.status.success(),
            "list --json failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let response: Response = serde_json::from_slice(&output.stdout).expect("list --json shape");
        response.sessions.expect("sessions")
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

fn session<'a>(sessions: &'a [SessionInfo], name: &str) -> &'a SessionInfo {
    sessions
        .iter()
        .find(|session| session.name == name)
        .unwrap_or_else(|| panic!("missing session {name}"))
}

#[tokio::test]
async fn start_all_starts_five_configured_sessions() {
    let harness = Harness::start().await;
    let output = harness.run(&["start", "--all"]);
    assert!(
        output.status.success(),
        "start --all failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let names: Vec<String> = harness
        .list_sessions()
        .into_iter()
        .map(|session| session.name)
        .collect();
    assert_eq!(names, ["claude", "codex", "grok", "kiro", "pi"]);
}

#[tokio::test]
async fn start_named_session_uses_config_argv_and_cwd() {
    let harness = Harness::start().await;
    let output = harness.run(&["start", "kiro"]);
    assert!(
        output.status.success(),
        "start kiro failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        String::from_utf8_lossy(&output.stdout).starts_with("kiro\t"),
        "start should print the session, got {}",
        String::from_utf8_lossy(&output.stdout)
    );

    let kiro = session(&harness.list_sessions(), "kiro").clone();
    assert_eq!(kiro.status, sumika_protocol::Status::Running);
    assert_eq!(kiro.argv, ["cat"]);
    assert!(kiro.pid.is_some());
    assert_eq!(std::path::Path::new(&kiro.cwd), harness.kiro_cwd.as_path());
}

#[tokio::test]
async fn start_live_name_does_not_spawn_a_second_child() {
    let harness = Harness::start().await;
    let first = harness.run(&["start", "kiro"]);
    assert!(first.status.success());
    let first_pid = session(&harness.list_sessions(), "kiro").pid;

    let second = harness.run(&["start", "kiro"]);
    assert!(second.status.success());
    let listed = harness.list_sessions();
    assert_eq!(listed.len(), 1);
    assert_eq!(session(&listed, "kiro").pid, first_pid);
}

fn list_text(harness: &Harness) -> String {
    let output = harness.run(&["list"]);
    assert!(
        output.status.success(),
        "list failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn list_json_value(harness: &Harness) -> serde_json::Value {
    let output = harness.run(&["list", "--json"]);
    assert!(output.status.success());
    serde_json::from_slice(&output.stdout).expect("list --json shape")
}

fn json_session<'a>(value: &'a serde_json::Value, name: &str) -> &'a serde_json::Value {
    value["sessions"]
        .as_array()
        .expect("sessions array")
        .iter()
        .find(|session| session["name"] == name)
        .unwrap_or_else(|| panic!("missing session {name}"))
}

#[tokio::test]
async fn recipe_project_surfaces_in_list_text_and_json() {
    let harness = Harness::start().await;
    assert!(harness.run(&["start", "kiro"]).status.success());
    assert!(harness.run(&["start", "claude"]).status.success());

    let kiro = session(&harness.list_sessions(), "kiro").clone();
    assert_eq!(kiro.project.as_deref(), Some("habitat"));
    let claude = session(&harness.list_sessions(), "claude").clone();
    assert_eq!(claude.project, None);

    let text = list_text(&harness);
    let kiro_line = text
        .lines()
        .find(|line| line.starts_with("kiro\t"))
        .expect("kiro list line");
    assert_eq!(kiro_line.split('\t').count(), 5);
    assert!(kiro_line.ends_with("\thabitat"), "line: {kiro_line:?}");
    let claude_line = text
        .lines()
        .find(|line| line.starts_with("claude\t"))
        .expect("claude list line");
    assert!(claude_line.ends_with('\t'), "line: {claude_line:?}");

    let value = list_json_value(&harness);
    assert_eq!(json_session(&value, "kiro")["project"], "habitat");
    assert!(
        json_session(&value, "claude").get("project").is_none(),
        "project must be absent, not null"
    );
}

#[tokio::test]
async fn project_flag_overrides_recipe_project() {
    let harness = Harness::start().await;
    let output = harness.run(&["start", "kiro", "--project", "override"]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        session(&harness.list_sessions(), "kiro").project.as_deref(),
        Some("override")
    );
}

#[tokio::test]
async fn project_flag_sets_project_without_recipe() {
    let harness = Harness::start().await;
    let output = harness.run(&["start", "grok", "--project", "fresh"]);
    assert!(output.status.success());
    assert_eq!(
        session(&harness.list_sessions(), "grok").project.as_deref(),
        Some("fresh")
    );
    let all = harness.run(&["start", "--all", "--project", "x"]);
    assert!(!all.status.success(), "start --all must reject --project");
}

#[tokio::test]
async fn start_argv_flag_overrides_config() {
    let harness = Harness::start().await;
    let output = harness.run(&["start", "kiro", "--", "sleep", "30"]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        session(&harness.list_sessions(), "kiro").argv,
        ["sleep", "30"]
    );
}
