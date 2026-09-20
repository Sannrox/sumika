# Vendor report adapters

These snippets call `sumika hook-report`, which maps a vendor hook payload to
`sumika report` and always exits 0. A broken or unknown payload does not
change session status and does not kill the child.

The daemon sets `SUMIKA_SESSION` to the session name when it starts the child.
`hook-report` uses that name. Do not scrape the PTY.

Do not hook Claude `idle_prompt` or `auth_success`. Do not invent a Kiro
approval event; Kiro approval stays unknown.

## Install (merge, do not replace)

Copy the `hooks` entries from the snippet into the existing file. Leave every
unrelated hook in place.

### Claude

Merge `claude.settings.json` into `~/.claude/settings.json` (or the project
`.claude/settings.json`). Under `hooks`, add the `Stop`, `Notification`
(`permission_prompt` only), and `PermissionRequest` groups. If `hooks` already
exists, append those groups; do not delete other events.

### Codex

Merge `codex.hooks.json` into `~/.codex/hooks.json` (or the project
`.codex/hooks.json`) the same way: add `Stop` and `PermissionRequest` without
removing other groups.

If you use the Codex `notify` command instead of (or in addition to) Stop,
merge `codex.notify.toml` into `~/.codex/config.toml` as an extra `notify`
entry. Do not drop other `notify` commands already listed.

### Grok

Copy `grok.hooks.json` into `~/.grok/hooks/` (or the project `.grok/hooks/`).
It adds `Stop` (idle) and `Notification` (blocked). Merge those groups if a
file already exists; do not delete other events.

### Kiro

CLI: merge the `stop` group from `kiro-cli.hooks.json` into the existing
kiro-cli agent hooks. IDE: merge the `Agent Stop` hook from `kiro.hooks.json`
into `.kiro/hooks/` without removing other files. There is no approval
snippet; Kiro has no documented approval event.

### Cursor

Merge `cursor.hooks.json` into `~/.cursor/hooks.json` (or the project
`.cursor/hooks.json`). Keep `"version": 1`. Add the `stop` group only. Do
not add `beforeSubmitPrompt`. Cursor has no documented CLI
permission/approval hook, so approval stays unknown.

The documented command is `agent`. That name collides with Grok's `agent` on
PATH. Sample config uses `cursor-agent` (Cursor's installer symlink) so
`start cursor` does not spawn Grok. If `agent` on PATH is Cursor, `argv =
["agent"]` is fine. Do not use Grok's `~/.grok/bin/agent`. Sumika does not
invent `--kind`. Restart is argv only; do not pass Cursor resume flags.

### Pi

Copy `pi/sumika-report.ts` into `~/.pi/agent/extensions/` (or the project
`.pi/extensions/`). It reports idle on `agent_end` / `agent_settled`. It
reports blocked on `permissions:ask` only when that event fires (the
`@pi-lab/permissions` extension). Without that extension, approval stays
unknown.
