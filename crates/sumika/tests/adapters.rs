use std::io::Write;
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
    notify: PathBuf,
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
        let notify = dir.path().join("notify.log");
        let bin = PathBuf::from(env!("CARGO_BIN_EXE_sumika"));
        let daemon = Command::new(&bin)
            .args(["daemon"])
            .env("SUMIKA_SOCK", &sock)
            .env("SUMIKA_NOTIFY_FILE", &notify)
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
            notify,
            _dir: dir,
        }
    }

    fn start_session(&self, name: &str) {
        let output = Command::new(&self.bin)
            .args(["start", name, "--", "cat"])
            .env("SUMIKA_SOCK", self.client.sock_path())
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    fn hook_report(&self, name: &str, payload: &str) -> std::process::Output {
        let mut child = Command::new(&self.bin)
            .args(["hook-report"])
            .env("SUMIKA_SOCK", self.client.sock_path())
            .env("SUMIKA_SESSION", name)
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        child
            .stdin
            .as_mut()
            .unwrap()
            .write_all(payload.as_bytes())
            .unwrap();
        child.wait_with_output().unwrap()
    }

    async fn session(&self, name: &str) -> sumika_protocol::SessionInfo {
        self.client
            .rpc(&Request::List)
            .await
            .unwrap()
            .sessions
            .unwrap()
            .into_iter()
            .find(|session| session.name == name)
            .unwrap()
    }
}

impl Drop for Harness {
    fn drop(&mut self) {
        let _ = self.daemon.kill();
        let _ = self.daemon.wait();
    }
}

#[tokio::test]
async fn child_inherits_sumika_session() {
    let harness = Harness::start().await;
    let out = harness.notify.parent().unwrap().join("session-name");
    let script = format!(
        "printf %s \"$SUMIKA_SESSION\" > {}; exec cat",
        out.display()
    );
    let started = Command::new(&harness.bin)
        .args(["start", "claude", "--", "sh", "-c", &script])
        .env("SUMIKA_SOCK", harness.client.sock_path())
        .output()
        .unwrap();
    assert!(started.status.success());
    let deadline = tokio::time::Instant::now() + Duration::from_secs(2);
    loop {
        if out.exists() {
            break;
        }
        if tokio::time::Instant::now() > deadline {
            panic!("child did not write SUMIKA_SESSION");
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert_eq!(std::fs::read_to_string(&out).unwrap(), "claude");
}

#[tokio::test]
async fn claude_stop_sets_idle_and_notifies_when_unfocused() {
    let harness = Harness::start().await;
    harness.start_session("claude");
    let out = harness.hook_report("claude", r#"{"hook_event_name":"Stop"}"#);
    assert!(out.status.success());
    let session = harness.session("claude").await;
    assert_eq!(session.status, Status::Idle);
    assert!(!session.focused);
    assert_eq!(
        std::fs::read_to_string(&harness.notify).unwrap(),
        "claude\tidle\n"
    );
}

#[tokio::test]
async fn claude_permission_prompt_sets_blocked() {
    let harness = Harness::start().await;
    harness.start_session("claude");
    let out = harness.hook_report(
        "claude",
        r#"{"hook_event_name":"Notification","notification_type":"permission_prompt"}"#,
    );
    assert!(out.status.success());
    assert_eq!(harness.session("claude").await.status, Status::Blocked);
    assert_eq!(
        std::fs::read_to_string(&harness.notify).unwrap(),
        "claude\tblocked\n"
    );
}

#[tokio::test]
async fn codex_stop_and_permission_request() {
    let harness = Harness::start().await;
    harness.start_session("codex");
    let stop = harness.hook_report("codex", r#"{"hook_event_name":"Stop"}"#);
    assert!(stop.status.success());
    assert_eq!(harness.session("codex").await.status, Status::Idle);
    let perm = harness.hook_report("codex", r#"{"hook_event_name":"PermissionRequest"}"#);
    assert!(perm.status.success());
    assert_eq!(harness.session("codex").await.status, Status::Blocked);
}

#[tokio::test]
async fn codex_notify_turn_complete_and_approval() {
    let harness = Harness::start().await;
    harness.start_session("codex");
    assert!(
        harness
            .hook_report("codex", r#"{"type":"agent-turn-complete"}"#)
            .status
            .success()
    );
    assert_eq!(harness.session("codex").await.status, Status::Idle);
    assert!(
        harness
            .hook_report("codex", r#"{"type":"approval-requested"}"#)
            .status
            .success()
    );
    assert_eq!(harness.session("codex").await.status, Status::Blocked);
}

#[tokio::test]
async fn grok_stop_sets_idle_and_notification_sets_blocked() {
    let harness = Harness::start().await;
    harness.start_session("grok");
    assert!(
        harness
            .hook_report("grok", r#"{"hookEventName":"Stop","sessionId":"g1"}"#)
            .status
            .success()
    );
    assert_eq!(harness.session("grok").await.status, Status::Idle);
    assert!(
        harness
            .hook_report("grok", r#"{"hookEventName":"Notification"}"#)
            .status
            .success()
    );
    assert_eq!(harness.session("grok").await.status, Status::Blocked);
}

#[tokio::test]
async fn kiro_stop_sets_idle_and_approval_stays_unknown() {
    let harness = Harness::start().await;
    harness.start_session("kiro");
    let first = harness.session("kiro").await;
    assert!(
        harness
            .hook_report("kiro", r#"{"hook_event_name":"PreToolUse"}"#)
            .status
            .success()
    );
    let after_unknown = harness.session("kiro").await;
    assert_eq!(after_unknown.status, Status::Running);
    assert_eq!(after_unknown.pid, first.pid);
    assert!(!harness.notify.exists());
    assert!(
        harness
            .hook_report("kiro", r#"{"hook_event_name":"stop"}"#)
            .status
            .success()
    );
    assert_eq!(harness.session("kiro").await.status, Status::Idle);
}

#[tokio::test]
async fn pi_agent_end_sets_idle_when_extension_payload_arrives() {
    let harness = Harness::start().await;
    harness.start_session("pi");
    assert!(
        harness
            .hook_report("pi", r#"{"type":"agent_settled"}"#)
            .status
            .success()
    );
    assert_eq!(harness.session("pi").await.status, Status::Idle);
}

#[tokio::test]
async fn broken_hook_leaves_unknown_and_keeps_the_child() {
    let harness = Harness::start().await;
    harness.start_session("claude");
    let first = harness.session("claude").await;
    let pid = first.pid;
    let out = harness.hook_report("claude", "not-json{{{");
    assert!(out.status.success());
    let again = harness.session("claude").await;
    assert_eq!(again.pid, pid);
    assert_eq!(again.status, Status::Running);
    assert!(!harness.notify.exists());
}

#[test]
fn snippets_are_merge_fragments() {
    let claude: serde_json::Value =
        serde_json::from_str(include_str!("../../../contrib/hooks/claude.settings.json")).unwrap();
    let hooks = claude["hooks"].as_object().unwrap();
    assert!(hooks.contains_key("Stop"));
    assert!(hooks.contains_key("Notification"));
    assert!(hooks.contains_key("PermissionRequest"));
    let matcher = hooks["Notification"][0]["matcher"].as_str().unwrap();
    assert_eq!(matcher, "permission_prompt");
    assert!(!hooks.contains_key("idle_prompt"));

    let codex: serde_json::Value =
        serde_json::from_str(include_str!("../../../contrib/hooks/codex.hooks.json")).unwrap();
    let hooks = codex["hooks"].as_object().unwrap();
    assert!(hooks.contains_key("Stop"));
    assert!(hooks.contains_key("PermissionRequest"));
    assert_eq!(hooks.len(), 2);

    let grok: serde_json::Value =
        serde_json::from_str(include_str!("../../../contrib/hooks/grok.hooks.json")).unwrap();
    let hooks = grok["hooks"].as_object().unwrap();
    assert!(hooks.contains_key("Stop"));
    assert!(hooks.contains_key("Notification"));
    assert!(!hooks.contains_key("PermissionRequest"));

    let kiro_cli: serde_json::Value =
        serde_json::from_str(include_str!("../../../contrib/hooks/kiro-cli.hooks.json")).unwrap();
    assert!(kiro_cli["hooks"].get("stop").is_some());
    assert!(kiro_cli["hooks"].get("PermissionRequest").is_none());

    let kiro: serde_json::Value =
        serde_json::from_str(include_str!("../../../contrib/hooks/kiro.hooks.json")).unwrap();
    assert_eq!(kiro["hooks"][0]["trigger"], "Agent Stop");
    let pi = include_str!("../../../contrib/hooks/pi/sumika-report.ts");
    assert!(pi.contains("agent_end"));
    assert!(pi.contains("agent_settled"));
    assert!(pi.contains("permissions:ask"));
}
