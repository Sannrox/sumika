# sumika

[![CI](https://github.com/Sannrox/sumika/actions/workflows/ci.yml/badge.svg)](https://github.com/Sannrox/sumika/actions/workflows/ci.yml)
[![License](https://img.shields.io/badge/license-Apache%202.0-blue.svg)](LICENSE)

**sumika** (棲みか, habitat) is a session switcher for interactive CLI agents.
The terminal is the window. Sumika owns the processes.

One chat on screen, full bleed. Closing the terminal does not kill the child.
Attach is a raw PTY. A second attach steals. Status is a hook or `unknown` —
never a screen scrape.

> **Status:** Phase 0. One PTY survives detach. This is pre-1.0 software, not
> published to crates.io. Build from source. The picker, launchd, and hook
> attention land in later phases; see [VISION.md](VISION.md).

## Why

Interactive coding agents die when the terminal dies. Multiplexers then grow
panes, workspaces, and mouse layout until they look like another product.

Sumika keeps a named child `{name, argv, cwd}` in a daemon-owned PTY. Detach
leaves the child. Every agent is the same shape: Kiro is `["kiro-cli"]`, the
same as Claude.

## Requirements

- macOS or Linux
- [Rust](https://rustup.rs/) with the toolchain in [`rust-toolchain.toml`](rust-toolchain.toml) (1.97.1, edition 2024)
- `python3` only for the resize integration test

## Quickstart

```bash
git clone https://github.com/Sannrox/sumika.git
cd sumika
cargo build --release

./target/release/sumika daemon
```

In another terminal:

```bash
./target/release/sumika start demo -- bash
./target/release/sumika attach demo
```

Close that terminal. The `bash` child stays. Reattach:

```bash
./target/release/sumika attach demo
./target/release/sumika list
./target/release/sumika kill demo
```

A second `attach` to the same name disconnects the previous client (steal).
There is no shared attach.

Override the socket with `--sock` or `SUMIKA_SOCK`. The default is
`$XDG_RUNTIME_DIR/sumika/sumika.sock` when that runtime dir is set, otherwise
a user-owned `0700` directory (`~/Library/Caches/sumika` on macOS,
`~/.cache/sumika` elsewhere). The daemon accepts only same-uid peers.

## CLI

```text
sumika [--sock PATH] <COMMAND>
```

| Command | Purpose |
| --- | --- |
| `daemon` | Run the PTY supervisor |
| `ping` | Check that the daemon is reachable |
| `start NAME -- ARGV...` | Spawn a named session |
| `list [--json]` | List sessions |
| `attach NAME` | Exclusive attach (steals) |
| `kill NAME [--force]` | Kill a session |

`start` takes `--cwd DIR`. Without it, the child's cwd is the client's current
directory. `start` on a live name returns that session and does not spawn a
second child. Restart is `kill` then `start` after the session is `dead`.

## What this is not

- Not a multiplexer. No panes, splits, workspaces, or mouse layout.
- Not a screen scraper. Report status comes from hooks, or it is `unknown`.
- Not a shared attach. One focused client per session.
- Not a vendor chat resume. Sumika keeps the process. After reboot, restart
  argv and use that tool's own resume.

## Related

Sumika is the interactive PTY habitat. It is not
[shikigami](https://github.com/Sannrox/shikigami) (headless runs),
[sekai-chisei](https://github.com/Sannrox/sekai-chisei) (governance),
[bugyo](https://github.com/Sannrox/bugyo) (Kiro GUI), or
[rusui](https://github.com/Sannrox/rusui) (machines).

Language: [CONTEXT.md](CONTEXT.md). Path: [VISION.md](VISION.md).
Week 0 locks: [docs/decisions/0001-week0.md](docs/decisions/0001-week0.md).

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md). Participation is governed by
[CODE_OF_CONDUCT.md](CODE_OF_CONDUCT.md). Report vulnerabilities through
[SECURITY.md](SECURITY.md).

## License

Apache License 2.0. See [LICENSE](LICENSE).
