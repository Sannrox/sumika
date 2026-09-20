# Vision

## Purpose

`sumika` (棲みか, habitat) is a vim-shaped session switcher for CLI agents. The terminal is the window. Sumika owns the processes.

One chat on screen, full bleed. A picker to jump to the others. Closing the terminal does not kill the child. A desktop ping fires when a turn ends or something needs approval, and only if that session is not focused.

Every agent is `{name, argv, cwd}`. Kiro is `["kiro-cli"]`, the same shape as Claude.

## Problem

Interactive coding agents die when the terminal dies. Multiplexers then grow panes, workspaces, and mouse layout until they look like another product. Status is scraped off the screen. The terminal emulator starts to own the process.

## Promise

An operator runs `sumika` in one terminal window. Five sessions live in the daemon. Attach is a raw PTY. Detach leaves the child. A documented chord always gets back to the picker. Hooks tell sumika `idle` or `blocked`; anything else is `unknown`.

## Principles

1. No tiling, no workspaces, no sidebar, no mouse layout, no `--kind`.
2. No screen scrape. Status is hook or `unknown`.
3. Do not emulate the agent UIs. Attach is a raw PTY.
4. The terminal never owns the process.
5. One focused client per session. Second attach steals.
6. If a feature would make this look like Herdr, it waits a year.

## Family

Sumika is the interactive PTY habitat. It is not rusui (machines), shikigami (headless runs), bugyo (Kiro GUI), or sekai-chisei (governance).

Sumika does not resume a vendor chat by magic. It keeps the process. After reboot, restart argv and use that tool's own resume if a hint was saved.

## Path

Phase 0 proves one PTY survives detach. Phase 1 is the daily driver for five agents, including Kiro, with a picker and launchd. Phase 2 is hook attention. Phase 3 is the terminal as a frame. Phase 4 is last-frame snapshot. Pane splits are not on the path.
