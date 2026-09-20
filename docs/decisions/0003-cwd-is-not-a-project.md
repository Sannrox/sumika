# cwd is not a project

- Status: accepted
- Date: 2026-09-20
- Source: [Issue #50](https://github.com/Sannrox/sumika/issues/50)
- Related: [Issue #24](https://github.com/Sannrox/sumika/issues/24), [Issue #2](https://github.com/Sannrox/sumika/issues/2)

A **Session** is the live PTY child `{name, argv, cwd}`. Uniqueness is `name`.
`cwd` is the directory the child starts in. It is not a grouping key, a switch
target, or a Project.

There is no Project type on the protocol, in config, or in the picker. The
**Picker** lists every Session and jumps by name. It is not a directory-scoped
view, a sticky folder, a tree, or a sidebar.

[VISION.md](../../VISION.md) principle 1 is not amended: no tiling, no
workspaces, no sidebar, no mouse layout, no `--kind`. A picker that switches
the current directory and then shows only sessions whose `cwd` matches that
directory is a workspace-shaped product. Same refusal as worktrees (#24).
Per-project config files stay a non-goal (#2).

`start --all` still starts every configured recipe. It does not mean “every
argv in this directory.”

## Alternatives considered

- Sticky picker scope by exact `cwd`: rejected. Invents a Project without
  naming one. Session identity would split between `name` and directory.
- Nested-directory matching (child `cwd` under the scoped path): rejected
  with the scope itself. No scope object, so no nesting rule.
- Live-only vs dead/configured rows inside a scope: rejected. The picker
  already lists live and dead rows in one flat list.
- `start` / picker `r` inheriting the scoped directory as `cwd`: rejected.
  `cwd` comes from the start recipe or the operator’s argv, not from a
  picker filter.
- Persist the scope like last-attached: rejected. Last-attached is a Session
  name. A persisted directory would be a second identity.
- Amend VISION principle 1 to allow directory scope: rejected. Principle 6
  still waits a year on anything that looks like that family of products.

## Consequences

Do not implement directory-scoped picker listing, directory switch commands,
or a Project type. Operators who want two children in one repo give them two
names and, if needed, the same `cwd` on each recipe. Jump keys and the
attention line stay name-based.

## Sources

- [CONTEXT.md](../../CONTEXT.md) Session and Picker vocabulary
- [VISION.md](../../VISION.md) principles 1 and 6
- [ontology/sumika-v1.json](../../ontology/sumika-v1.json) Session, Picker,
  `jumps_to`
