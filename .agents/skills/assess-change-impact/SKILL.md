---
name: assess-change-impact
description: Assess a proposed or implemented sumika change across daemon, protocol, attach, CLI, and socket boundaries. Use when scoping an Issue, planning tests, reviewing a diff, or identifying documentation, compatibility, and security obligations.
---

# Assess Change Impact

Build an evidence-backed impact map before implementation or review.

## Procedure

1. Read the linked Issue or request, `VISION.md`, `CONTEXT.md`,
   `docs/decisions/0001-week0.md`, and the relevant code. For a diff, inspect
   every changed file and its direct callers or implementors. Complete when the
   claimed outcome and actual change surface are both known.
2. Trace applicable boundaries:
   - daemon-owned PTY versus the attaching client;
   - attach exclusivity (steal) versus detach (child stays);
   - JSON-lines control versus raw PTY attach bytes;
   - `sumika-protocol` types versus `sumika-daemon` supervisor versus
     `sumika-ctl` / `sumika` CLI;
   - Unix socket path, `0700` parent, and same-uid accept;
   - report/hook status versus screen scrape;
   - one installed binary; no panes, splits, or `--kind`.
   Complete when each applicable boundary has an owner and expected invariant.
3. Identify compatibility obligations. Include old clients, socket path
   changes, error/exit codes, and what happens to a live child on kill, steal,
   or daemon restart. Complete when process-leak and stolen-attach paths are
   accounted for.
4. Map evidence to risk: unit tests for protocol/client logic; integration
   tests in `crates/sumika/tests/` for attach/detach/steal/reap; do not add
   tests that mock PTY supervisor internals. Complete when every material risk
   has a proposed check or an explicit residual uncertainty.
5. Determine durable artifacts that must change: README, CHANGELOG, ADR, or a
   repository Skill. Complete when no artifact is proposed merely to record
   temporary planning.

## Output

Return a compact matrix with columns:

| Surface | Evidence found | Required change/check | Risk if missed |
| --- | --- | --- | --- |

Then list scope boundaries, blocking questions, and the smallest safe PR split.
Do not approve an architecture, perform a full security audit, or claim
attach/steal behavior without inspecting the implementations.
