# sumika

[![CI](https://github.com/Sannrox/sumika/actions/workflows/ci.yml/badge.svg)](https://github.com/Sannrox/sumika/actions/workflows/ci.yml)
[![License](https://img.shields.io/badge/license-Apache%202.0-blue.svg)](LICENSE)

**sumika** (棲みか, habitat) is a session switcher for interactive CLI agents.
The terminal is the window. Sumika owns the processes.

One chat on screen, full bleed. Closing the terminal does not kill the child.
Attach is a raw PTY. A second attach steals. Status is a hook or `unknown` —
never a screen scrape.

> **Status:** Phase 1 daily driver. One terminal window runs `sumika`; the
> daemon is the workspace. This is pre-1.0 software, not published to
> crates.io. Build from source. Hook attention lands later; see
> [VISION.md](VISION.md).

## Daily driver

Use one terminal window whose command is `sumika`. The picker is how you
jump. Keys live in that TUI, not in the terminal emulator: `j`/`k`, Enter,
`q`, configured jump keys, `r` on a dead row, and the detach chord
(default `Ctrl-\` then `Ctrl-b`) back to the picker. The launchd user agent
owns the daemon; the daemon is the workspace. Closing the window leaves
every child running. Clicking a notification is not required.

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

Named sessions live in `~/.config/sumika/config.toml` (override `--config` or
`SUMIKA_CONFIG`, same precedence as the socket: flag, then env, then default).
`sumika start kiro` uses that record when argv is omitted. `sumika start --all`
starts every configured session. Live names stay idempotent.

```toml
[[sessions]]
name = "claude"
argv = ["claude"]
key = "c"

[[sessions]]
name = "grok"
argv = ["grok"]
key = "g"

[[sessions]]
name = "kiro"
argv = ["kiro-cli"]
cwd = "/path/to/project"
key = "k"

[[sessions]]
name = "pi"
argv = ["pi"]
key = "p"

[[sessions]]
name = "codex"
argv = ["codex"]
key = "x"
```

`key` is a one-character jump in the picker (`c`, `g`, `k`, `p`, `x` in the
sample). `j`/`k` still move unless that character is a jump key; use the arrow
keys to move in that case.

## CLI

Bare `sumika` opens a full-screen picker of daemon sessions. `j`/`k` move,
Enter attaches (raw PTY, full bleed), `q` leaves the picker and does not kill
children. Configured `key` values jump. `r` on a dead row starts that session
from config. While attached, the client intercepts a two-key detach chord
(default `Ctrl-\` then `Ctrl-b`) and returns to the picker; those bytes are
not sent to the child. Override with `detach_chord = ["C-\\", "C-b"]` in
config. Closing the window still leaves the child.

```text
sumika [--sock PATH] [--config PATH] [COMMAND]
```

| Command | Purpose |
| --- | --- |
| *(none)* | Open the session picker |
| `daemon` | Run the PTY supervisor |
| `ping` | Check that the daemon is reachable |
| `start NAME [-- ARGV...]` | Spawn a named session (argv from config when omitted) |
| `start --all` | Start every session in the config |
| `list [--json]` | List sessions |
| `attach NAME` | Exclusive attach (steals) |
| `kill NAME [--force]` | Kill a session |
| `doctor [--json]` | Socket, reachability, launchd, config, session pids |
| `report NAME idle\|blocked\|running` | Hook attention status |

`sumika report` stores hook status on the session. If that session is not the
focused attach, sumika fires a macOS notification with the name and status —
never PTY contents. Absence of a report is `unknown`. Adapters fail open.

On macOS the daily-driver owner is a launchd user agent
(`contrib/launchd/com.sumika.daemon.plist`). If the default socket is missing,
`sumika` installs and bootstraps that agent so the daemon is not a child of the
TUI. `launchctl bootstrap gui/$UID ~/Library/LaunchAgents/com.sumika.daemon.plist`
is the one-shot install. A systemd `--user` unit ships in
`contrib/systemd/sumika.service` but is not the daily-driver claim.

`doctor` is non-zero when the daemon is unreachable. It never prints PTY
contents.

`start` takes `--cwd DIR`. Without it, the child's cwd is the config `cwd` if
present, otherwise the client's current directory. Flags override config.
`start` on a live name returns that session and does not spawn a
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
