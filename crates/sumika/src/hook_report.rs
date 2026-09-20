use serde_json::Value;
use sumika_protocol::Status;

/// Map a vendor hook/notify payload to a Report status.
/// Unknown or ignored events (idle_prompt, auth_success, Kiro approval
/// absence, …) return None.
pub fn status_from_payload(payload: &Value) -> Option<Status> {
    let mut idle = None;
    for raw in event_strings(payload) {
        match map_event(&raw, payload) {
            Some(Status::Blocked) => return Some(Status::Blocked),
            Some(Status::Idle) => idle = Some(Status::Idle),
            Some(_) | None => {}
        }
    }
    idle
}

fn event_strings(payload: &Value) -> Vec<String> {
    ["type", "hook_event_name", "hookEventName", "trigger"]
        .iter()
        .filter_map(|key| payload.get(*key).and_then(Value::as_str))
        .map(str::to_string)
        .collect()
}

fn normalize(raw: &str) -> String {
    raw.trim()
        .to_ascii_lowercase()
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .collect()
}

fn map_event(raw: &str, payload: &Value) -> Option<Status> {
    match normalize(raw).as_str() {
        "agentturncomplete" | "stop" | "agentstop" | "agentend" | "agentsettled" => {
            Some(Status::Idle)
        }
        "approvalrequested" | "permissionrequest" | "permissionsask" => Some(Status::Blocked),
        "notification" => notification_status(payload),
        _ => None,
    }
}

fn notification_status(payload: &Value) -> Option<Status> {
    let kind = payload
        .get("notification_type")
        .or_else(|| payload.get("notificationType"))
        .or_else(|| payload.get("matcher"))
        .and_then(Value::as_str)
        .unwrap_or("");
    match normalize(kind).as_str() {
        "idleprompt" | "authsuccess" => None,
        "permissionprompt" => Some(Status::Blocked),
        "" => Some(Status::Blocked),
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
    fn grok_stop_and_notification() {
        assert_eq!(
            status_from_payload(&json!({"hookEventName": "Stop", "sessionId": "g1"})),
            Some(Status::Idle)
        );
        assert_eq!(
            status_from_payload(&json!({"hookEventName": "Notification"})),
            Some(Status::Blocked)
        );
    }

    #[test]
    fn kiro_stop_is_idle_and_approval_is_unknown() {
        assert_eq!(
            status_from_payload(&json!({"hook_event_name": "stop"})),
            Some(Status::Idle)
        );
        assert_eq!(
            status_from_payload(&json!({"trigger": "Agent Stop"})),
            Some(Status::Idle)
        );
        assert_eq!(
            status_from_payload(&json!({"hook_event_name": "PreToolUse"})),
            None
        );
    }

    #[test]
    fn pi_agent_end_and_permissions_ask() {
        assert_eq!(
            status_from_payload(&json!({"type": "agent_end"})),
            Some(Status::Idle)
        );
        assert_eq!(
            status_from_payload(&json!({"type": "agent_settled"})),
            Some(Status::Idle)
        );
        assert_eq!(
            status_from_payload(&json!({"type": "permissions:ask"})),
            Some(Status::Blocked)
        );
    }

    #[test]
    fn garbage_is_ignored() {
        assert_eq!(status_from_payload(&json!({"nope": true})), None);
    }
}
