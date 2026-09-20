# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## Unreleased

### Added

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
