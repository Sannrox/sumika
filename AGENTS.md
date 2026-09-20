# Repository Guidelines

`sumika` is a Rust 2024 workspace: a Unix-socket daemon that owns PTYs, and a CLI that starts, lists, attaches, and kills sessions.

- `crates/sumika-protocol` — JSON-lines types and socket path
- `crates/sumika-daemon` — PTY supervisor
- `crates/sumika-ctl` — client
- `crates/sumika` — `sumika` binary

Read [CONTEXT.md](CONTEXT.md), [VISION.md](VISION.md), and [docs/decisions/0001-week0.md](docs/decisions/0001-week0.md) before changing boundaries. No panes, no screen scrape, no second attach that shares.

Human contribution flow: [CONTRIBUTING.md](CONTRIBUTING.md). Vulnerabilities: [SECURITY.md](SECURITY.md). Conduct: [CODE_OF_CONDUCT.md](CODE_OF_CONDUCT.md).

## Commands

- `cargo fmt`
- `cargo test --locked`
- `cargo clippy --all-targets --locked -- -D warnings`

Integration tests in `crates/sumika/tests/` are the attach/detach/steal/reap seam. Do not add tests that mock the PTY supervisor internals.

Portable product vocabulary lives in `ontology/sumika-v1.json`. Do not commit `.sekai/` or `knowledge.db`.

Public issues, PRs, logs, and docs must not include private paths, credentials, session contents, or internal environment names.

## Closeout

`cargo test` is not a substitute for structured review. Run `autoreview --mode local` from the shared skill install before ship. Do not vendor the helper into this repo.
