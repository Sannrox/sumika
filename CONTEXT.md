# Sumika

Sumika is the habitat that holds interactive CLI agent sessions so they outlive the terminal window.

## Language

**Sumika**:
The habitat. A local daemon plus the command that attaches to it.
_Avoid_: agentsd, multiplexer, workspace, tmux, Herdr

**Session**:
A named child process `{name, argv, cwd}` living in a PTY owned by sumika.
_Avoid_: pane, tab, kind, run, agent type

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

**Picker**:
The full-screen list used to jump between sessions. Not a layout.
_Avoid_: sidebar, tiles, workspaces
