# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## Unreleased

### Added

- Decision: the picker is not a directory scope. Session uniqueness stays
  `name`. `cwd` is the child's working directory, not a project
  ([`docs/decisions/0003-cwd-is-not-a-project.md`](docs/decisions/0003-cwd-is-not-a-project.md)).
- Picker attention: blocked rows sort first, a status line names who needs
  you (`kiro !  cursor ·`). Desktop banners are off unless `SUMIKA_NOTIFY=os`.
  `SUMIKA_NOTIFY_FILE` remains the test sink.
- Cursor CLI sample session (`cursor-agent`) and report adapter
  (`contrib/hooks/cursor.hooks.json`). `stop` → idle. Approval stays
  unknown. `agent` on PATH may be Grok; prefer `cursor-agent`.
- Persist `resume_hint` from confirmed Stop payloads (`claude`, `grok`,
  `codex`) and pass the documented flag on restart of a dead session.
  `kiro-cli`, `pi`, and unknown vendors keep argv unchanged.
- Detach chord default is `C-b` then `q` (copy mode `C-b` then `y`). `C-\\`
  is no longer the prefix — it is awkward on many keyboard layouts.
- Picker jump keys require Space first (`spc k`), so `j`/`k` always move.
  Detach restores the client screen (leave alt-screen). SIGINT/SIGQUIT are
  ignored while the TUI is up so `C-\\` is the chord, not process quit.
  launchd inherits a login PATH so `start --all` can spawn Homebrew/local
  agent binaries.
- Linux daily driver: systemd `--user` unit owns `sumika daemon`. `doctor`
  reports whether the user unit is active. Desktop banners stay opt-in
  (`SUMIKA_NOTIFY=os`).
- Copy mode over the session line ring (prefix then `y`). Yank goes to the
  system clipboard. Attach stays a raw PTY otherwise; steal and the child
  are unchanged.
- Picker help page (`?`) and `sumika keys` listing move, attach, quit,
  restart, jump keys, glyphs, and the detach chord.
- Operator daemon log (`$XDG_STATE_HOME/sumika/daemon.log`, override
  `SUMIKA_LOG` / `SUMIKA_STATE_DIR`): listen, start, steal, reap, report,
  notify, errors. Never PTY contents.
- Bounded line-ring scrollback on attach (10k lines), replayed before the
  last VT frame. Detached output still does not block the child.
- Last-frame snapshot on attach: the client first sees the last VT frame the
  daemon kept (`termwiz`). Detached output does not block the child.
- Grok, Kiro, and Pi report adapters. Grok Stop → idle, Notification →
  blocked. Kiro Stop / Agent Stop → idle only. Pi `agent_end` /
  `agent_settled` → idle; `permissions:ask` only when that extension is
  present.
- Claude and Codex report adapters (`contrib/hooks/`, `sumika hook-report`).
  Merge into existing vendor settings. Stop → idle; permission/approval →
  blocked. Broken payloads leave status unchanged and keep the child.
- Picker glyphs from hook status: `!` blocked, `·` idle, `…` running,
  `?` unknown, `✗` dead.
- Last-attached name under the XDG state dir. `sumika attach` with no name
  reattaches that session; bare `sumika` still opens the picker.
- `sumika report NAME idle|blocked|running` stores hook status and notifies
  on macOS when that session is not focused. Notifications never include
  PTY contents.
- README / VISION one-window daily driver: `sumika` is the window command,
  keys live in the TUI (including the detach chord), the daemon is the
  workspace.
- launchd user agent (`com.sumika.daemon`) as the macOS daemon owner, with
  `sumika doctor [--json]` and in-tree systemd `--user` unit.
- Detach chord (default `Ctrl-\` then `Ctrl-b`) returns from attach to the
  picker without forwarding those bytes or killing the child.
- Bare `sumika` opens a full-screen picker (`j`/`k`/Enter/`q`, configured `key`
  jumps, `r` restarts a dead row from config). Children stay running when the
  picker exits.
- Named session config (`~/.config/sumika/config.toml`, override `--config` or
  `SUMIKA_CONFIG`). `sumika start <name>` uses `{name, argv, cwd?, key?}` when
  argv is omitted; `sumika start --all` starts every configured session. Live
  names stay idempotent.
- Phase 0 habitat: a Unix-socket daemon owns PTYs, and `sumika` starts, lists,
  attaches, and kills named sessions. Detach leaves the child. A second attach
  steals. Dead sessions stay dead until `start`.
- Project-owned agent Skills under `.agents/skills/` (shape, frontier,
  deliver, verify, ontology, docs). Personal Skills remain gitignored.
- Decision on vendor resume-by-id flags after reboot
  ([`docs/decisions/0002-vendor-resume-flags.md`](docs/decisions/0002-vendor-resume-flags.md)).
