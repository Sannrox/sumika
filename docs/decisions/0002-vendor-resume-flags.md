# Vendor resume flags

- Status: accepted
- Date: 2026-09-20
- Source: [Issue #14](https://github.com/Sannrox/sumika/issues/14)

A **Session** is the live PTY child `{name, argv, cwd}`. A vendor chat identity is external. They are not the same object.

`resume_hint` is an optional opaque string on a Session. Store it only when a confirmed Stop (or equivalent turn-complete hook) payload provides an id **and** a confirmed resume-by-id shape exists. Sumika never invents chat resume. It keeps the process. After reboot it restarts argv and, when both facts are confirmed, appends that shape. If either fact is unknown, do not pass a flag. Do not scrape the PTY. Do not pass picker or “most recent in this directory” forms (`--continue`, `--resume` with no id, `--last`).

| argv | Resume-by-id shape | Stop provides an id? | `#18` may store? |
| --- | --- | --- | --- |
| `claude` | `--resume <session-id>` (`-r` same). Bare `--resume` is a picker. `--continue` / `-c` is most-recent, not an id. | Yes. Common Stop stdin field `session_id`. | Yes. Append `--resume` and the opaque id. |
| `grok` | `--resume <session-id>` (`-r` same). Bare `--resume` is most-recent. `-s` / `--session-id` names a **new** session; it does not resume. | Yes. Stop stdin includes `sessionId`; hook env `GROK_SESSION_ID`. | Yes. Append `--resume` and the opaque id. |
| `kiro-cli` | `--resume-id <SESSION_ID>` on `kiro-cli chat` (also `kiro-cli --resume-id` for cloud). `--resume` / `-r` is most-recent. `--resume-picker` is a picker. | Unknown. Current Stop stdin is not documented; older CLI copy and a still-open issue conflict. | No. Restart argv only. |
| `pi` | `--session <path\|id>` (path or partial UUID). `-r` / `--resume` is a picker and does not take an id. `-c` / `--continue` is most-recent. | No first-party Stop/notify JSON with an id. Extensions can read the id; that is not a Stop payload. | No. Restart argv only. |
| `codex` | Subcommand `codex resume <SESSION_ID>` (UUID or session name). Not a `--resume` flag. Bare `codex resume` is a picker. `--last` is most-recent. | Yes. Common Stop stdin field `session_id`. | Yes. Restart as `codex resume` plus the opaque id, not `codex --resume`. |

## Alternatives considered

- Pass `--continue` / `--resume` / `--last` with no id after reboot: rejected. That invents “most recent chat.”
- Parse exit banners or JSON-mode stdout for an id: rejected. Screen scrape.
- Pass `--session-id` on grok: rejected. First-party sessions page says it does not resume.
- Treat pi `--resume` like claude `--resume <id>`: rejected. First-party CLI says `-r` opens a picker.
- Store a hint when only the flag or only the Stop id is confirmed: rejected. Both are required.

## Consequences for [#18](https://github.com/Sannrox/sumika/issues/18)

Store `resume_hint` only for `claude`, `grok`, and `codex`, and only from a confirmed Stop payload. On `start` / picker `r` of a dead session, pass the shape in the table. `kiro-cli` and `pi` restart argv unchanged. Unknown vendors get no extra argv. An integration test with a fake Stop payload must cover that.

## Sources

- Claude CLI: https://code.claude.com/docs/en/cli-reference
- Claude sessions: https://code.claude.com/docs/en/sessions
- Claude hooks: https://code.claude.com/docs/en/hooks
- Grok CLI: https://docs.x.ai/build/cli/reference
- Grok sessions: https://docs.x.ai/build/features/sessions
- Grok hooks: https://docs.x.ai/build/features/hooks
- kiro-cli chat: https://kiro.dev/docs/cli/chat/session-management.md
- kiro-cli commands: https://kiro.dev/docs/reference/cli-commands.md
- Kiro hook types: https://kiro.dev/docs/hooks/types.md
- Pi sessions: https://pi.dev/docs/latest/sessions
- Pi CLI: https://github.com/earendil-works/pi/blob/main/packages/coding-agent/docs/usage.md
- Pi extensions: https://pi.dev/docs/latest/extensions
- Codex CLI: https://learn.chatgpt.com/docs/developer-commands?surface=cli
- Codex hooks: https://learn.chatgpt.com/docs/hooks
