# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## Unreleased

### Added

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
