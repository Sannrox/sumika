use serde_json::Value;
use sumika_protocol::Status;

/// Map a vendor hook/notify payload to a Report status.
/// Unknown or ignored events (idle_prompt, auth_success, …) return None.
pub fn status_from_payload(payload: &Value) -> Option<Status> {
    if let Some(kind) = payload.get("type").and_then(Value::as_str) {
        return match kind {
            "agent-turn-complete" => Some(Status::Idle),
            "approval-requested" => Some(Status::Blocked),
            _ => None,
        };
    }
    let event = payload
        .get("hook_event_name")
        .and_then(Value::as_str)
        .unwrap_or("");
    match event {
        "Stop" => Some(Status::Idle),
        "PermissionRequest" => Some(Status::Blocked),
        "Notification" => notification_status(payload),
        _ => None,
    }
}

fn notification_status(payload: &Value) -> Option<Status> {
    let kind = payload
        .get("notification_type")
        .or_else(|| payload.get("matcher"))
        .and_then(Value::as_str)
        .unwrap_or("");
    match kind {
        "permission_prompt" => Some(Status::Blocked),
        "idle_prompt" | "auth_success" => None,
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn claude_stop_is_idle() {
        let payload = json!({"hook_event_name": "Stop", "session_id": "abc"});
        assert_eq!(status_from_payload(&payload), Some(Status::Idle));
    }

    #[test]
    fn claude_permission_prompt_is_blocked() {
        let payload = json!({
            "hook_event_name": "Notification",
            "notification_type": "permission_prompt"
        });
        assert_eq!(status_from_payload(&payload), Some(Status::Blocked));
    }

    #[test]
    fn claude_permission_request_is_blocked() {
        let payload = json!({"hook_event_name": "PermissionRequest", "tool_name": "Bash"});
        assert_eq!(status_from_payload(&payload), Some(Status::Blocked));
    }

    #[test]
    fn claude_idle_prompt_and_auth_success_are_ignored() {
        let idle = json!({
            "hook_event_name": "Notification",
            "notification_type": "idle_prompt"
        });
        let auth = json!({
            "hook_event_name": "Notification",
            "notification_type": "auth_success"
        });
        assert_eq!(status_from_payload(&idle), None);
        assert_eq!(status_from_payload(&auth), None);
    }

    #[test]
    fn codex_stop_and_turn_complete_are_idle() {
        assert_eq!(
            status_from_payload(&json!({"hook_event_name": "Stop"})),
            Some(Status::Idle)
        );
        assert_eq!(
            status_from_payload(&json!({"type": "agent-turn-complete"})),
            Some(Status::Idle)
        );
    }

    #[test]
    fn codex_permission_and_approval_are_blocked() {
        assert_eq!(
            status_from_payload(&json!({"hook_event_name": "PermissionRequest"})),
            Some(Status::Blocked)
        );
        assert_eq!(
            status_from_payload(&json!({"type": "approval-requested"})),
            Some(Status::Blocked)
        );
    }

    #[test]
    fn garbage_is_ignored() {
        assert_eq!(status_from_payload(&json!({"nope": true})), None);
    }
}
