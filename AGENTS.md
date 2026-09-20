# Repository Guidelines

`sumika` is a Rust 2024 workspace: a Unix-socket daemon that owns PTYs, and a CLI that starts, lists, attaches, and kills sessions.

- `crates/sumika-protocol` — JSON-lines types and socket path
- `crates/sumika-daemon` — PTY supervisor
- `crates/sumika-ctl` — client
- `crates/sumika` — `sumika` binary

Read [CONTEXT.md](CONTEXT.md), [VISION.md](VISION.md), and [docs/decisions/0001-week0.md](docs/decisions/0001-week0.md) before changing boundaries. No panes, no screen scrape, no second attach that shares.

Human contribution flow: [CONTRIBUTING.md](CONTRIBUTING.md). Vulnerabilities: [SECURITY.md](SECURITY.md). Conduct: [CODE_OF_CONDUCT.md](CODE_OF_CONDUCT.md).

GitHub Issues are the planning source of truth. Project-owned Skills live
under `.agents/skills/`. Personal Skills remain gitignored.

## Commands

- `cargo fmt`
- `cargo test --locked`
- `cargo clippy --all-targets --locked -- -D warnings`

Integration tests in `crates/sumika/tests/` are the attach/detach/steal/reap seam. Do not add tests that mock the PTY supervisor internals.

Portable product vocabulary lives in `ontology/sumika-v1.json`. Consult it
with the project-local `sekai-ontology` Skill. Do not commit `.sekai/` or
`knowledge.db`.

Public issues, PRs, logs, and docs must not include private paths, credentials, session contents, or internal environment names.

## Parallel delivery lanes

A delivery lane is one Issue, one branch, one isolated checkout, one Pull
Request, and one owner. Claims live on GitHub. Claim with
`bash .agents/skills/deliver-ready-issue/scripts/issue-lane.sh claim <issue>`
before implementing under Publish or Land. Agents never switch, reset, or
stash the primary checkout. Collision surfaces: `sumika-protocol`, the PTY
supervisor, attach exclusivity, and the Unix socket path. The executable lead
procedure is `.agents/skills/deliver-ready-issue/references/parallel-delivery.md`.

## Closeout

`cargo test` is not a substitute for structured review. For non-trivial work
that will be committed or opened as a PR:

1. Run `verify-change` (focused tests, then `cargo fmt --check`,
   `cargo test --locked`, `cargo clippy --all-targets --locked -- -D warnings`).
2. Run `autoreview --mode local` from the shared skill install before ship.
   Do not vendor the helper into this repo.
