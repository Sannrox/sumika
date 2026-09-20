# Claude and Codex report adapters

These snippets call `sumika hook-report`, which maps a vendor hook payload to
`sumika report` and always exits 0. A broken or unknown payload does not
change session status and does not kill the child.

The daemon sets `SUMIKA_SESSION` to the session name when it starts the child.
`hook-report` uses that name. Do not scrape the PTY.

Do not hook Claude `idle_prompt` or `auth_success`.

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
