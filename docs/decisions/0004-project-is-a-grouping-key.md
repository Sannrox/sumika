# Project is a session grouping key

- Status: accepted
- Date: 2026-09-22
- Source: [Issue #60](https://github.com/Sannrox/sumika/issues/60) (direction B)
- Supersedes: [0003-cwd-is-not-a-project.md](0003-cwd-is-not-a-project.md)

A **Session** gains an optional `project` name, set at `start` from the
recipe or argv. Uniqueness stays `name`. `cwd` keeps its
[0003](0003-cwd-is-not-a-project.md) meaning: the directory the child starts
in, nothing more.

`project` is a grouping key for display. The protocol carries it
(`SessionInfo`), config recipes accept it, and the picker groups by it.
Grouping never filters the listing and never becomes a second identity: the
picker still lists every session and jumps by name.

Non-goals carried forward: worktree lifecycle and branch mapping (#24
stands), per-project config files (#2 stands), and directory-as-identity.
A directory-scoped picker view remains a workspace-shaped product.

## Alternatives considered

- Prefix naming convention (`repo/session`, group by prefix): rejected by
  the #60 direction call. No new type, but the convention is unenforceable
  and leaks identity into names.
- Directory scope as filter: rejected with ADR-0003. No scope object, so no
  nesting rule and no persisted directory identity.

## Consequences

- Protocol addition: old clients ignore the field.
- Config schema addition: optional `project` on recipes.
- Picker grouping UI: group headers, attention order unchanged inside groups.
- Vocabulary: `Session` gains a project relation; update
  `ontology/sumika-v1.json` when the type ships, not before.
- [VISION.md](../../VISION.md) principle 1 is not amended: no tiling, no
  workspaces, no sidebar. Group headers are not a sidebar.

## Sources

- [CONTEXT.md](../../CONTEXT.md) Session and Picker vocabulary
- [Issue #24](https://github.com/Sannrox/sumika/issues/24) worktrees refusal
- [Issue #60](https://github.com/Sannrox/sumika/issues/60) proposal and
  direction-B decision
