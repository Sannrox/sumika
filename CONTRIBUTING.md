# Contributing

Thanks for helping improve **sumika**. This is Phase 0: focused changes that
keep one PTY alive across detach are more useful than multiplexer features.

Participation is governed by [CODE_OF_CONDUCT.md](CODE_OF_CONDUCT.md).
Report exploitable vulnerabilities through [SECURITY.md](SECURITY.md), not in
a public issue or pull request.

## Before you start

- Search existing issues and pull requests before proposing overlapping work.
- Read [VISION.md](VISION.md), [CONTEXT.md](CONTEXT.md),
  [docs/decisions/0001-week0.md](docs/decisions/0001-week0.md), and
  [docs/decisions/0003-cwd-is-not-a-project.md](docs/decisions/0003-cwd-is-not-a-project.md).
- Open an issue before changing attach exclusivity, status reporting, or the
  daemon/CLI crate split.

## Development setup

Requirements: the pinned Rust toolchain from
[`rust-toolchain.toml`](rust-toolchain.toml), and `python3` for the resize
integration test.

```bash
git clone https://github.com/Sannrox/sumika.git
cd sumika
cargo fmt --check
cargo test --locked
cargo clippy --all-targets --locked -- -D warnings
```

## Tests

- Protocol and client unit tests live beside the crate they cover.
- Attach, detach, steal, and reap coverage lives in
  `crates/sumika/tests/`. That is the public seam. Do not add tests that mock
  the PTY supervisor internals.
- Default `cargo test` must stay green without extra services.

## Architecture rules

1. No tiling, workspaces, sidebar, mouse layout, or `--kind`.
2. No screen scrape. Status is hook or `unknown`.
3. Attach is a raw PTY. Do not emulate the agent UIs.
4. One focused client per session. Second attach steals.
5. Dead stays dead until `start`. No auto-respawn.
6. One installed binary.

Agent instructions: [AGENTS.md](AGENTS.md). Project-owned Skills live under
[`.agents/skills/`](.agents/skills/). Personal Skills stay gitignored.

## Pull requests

1. Keep the change focused (one outcome per PR).
2. Include tests for behavior changes at the attach/detach/steal/reap seam
   when that path moves.
3. Run the three commands above and include the results in the PR description.
4. Update [CHANGELOG.md](CHANGELOG.md) for user-visible changes.
5. Link an issue when one exists.

Commit subjects: short imperative. Conventional Commits welcome
(`feat:`, `fix:`, `docs:`, `chore:`).

## License

By contributing, you agree that your contributions are licensed under the
Apache License 2.0 ([LICENSE](LICENSE)).
