# Sumika

Sumika is the habitat that holds interactive CLI agent sessions so they outlive the terminal window.

## Language

**Sumika**:
The habitat. A local daemon plus the command that attaches to it.
The daemon writes one operator log without PTY contents.
_Avoid_: agentsd, multiplexer, workspace, tmux, Herdr

**Session**:
A named child process `{name, argv, cwd}` living in a PTY owned by sumika.
Uniqueness is `name`. `cwd` is the child's working directory, not a project.
The daemon keeps a last VT frame and a bounded line ring for attach.
_Avoid_: pane, tab, kind, run, agent type, project

**Attach**:
The exclusive focused client of one session. Bytes are a raw PTY; the child is not the client's child.
_Avoid_: share, mirror, split

**Detach**:
The client leaves. The child stays.
_Avoid_: close, exit, kill (those are different)

**Steal**:
A new attach displaces the previous attach. The old client is disconnected.
_Avoid_: share, refuse, broadcast

**Report**:
A hook-originated attention signal (`idle`, `blocked`, `running`) for a session. Absence is `unknown`.
_Avoid_: screen scrape, parser, idle_prompt

**Copy mode**:
A client overlay over the session line ring. Yank copies a line. Attach is a raw PTY otherwise.
_Avoid_: tmux copy-mode as identity, screen scrape, agent TUI clone

**Picker**:
The full-screen list used to jump between sessions. Not a layout.
_Avoid_: sidebar, tiles, workspaces, directory scope
