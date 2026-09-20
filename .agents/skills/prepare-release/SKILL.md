---
name: prepare-release
description: Prepare a sumika release by auditing version scope, compatibility, validation, artifacts, and release notes. Use when a maintainer asks for release readiness, a version bump plan, or a draft GitHub Release.
---

# Prepare Release

Assemble decision-ready release evidence. Do not tag, push, publish, or alter
GitHub state unless the maintainer explicitly authorizes that action.

## Procedure

1. Identify the target version, base tag, target commit, and milestone or merged
   PR range. Read `Cargo.toml`, the release workflow, and open release-blocking
   Issues. Complete when the exact release contents are bounded.
2. Classify changes as `Added`, `Changed`, `Fixed`, `Security`, or `Migration`.
   Check SemVer fit; before `1.0`, call out all public breaking changes even when
   they fit a minor bump. Complete when every user-visible merged change is
   represented once.
3. Audit release impact:
   - workspace `Cargo.toml` version and lockfile consistency;
   - CLI commands, flags, and exit codes;
   - attach/steal/detach and kill/reap behavior;
   - Unix socket path, `0700` parent, and same-uid accept;
   - `CHANGELOG.md` versus merged user-visible changes;
   - crates.io: keep `publish = false` until a maintainer authorizes
     publication.
   Complete when every applicable item is resolved or a named blocker.
4. Use `verify-change` for the full local gates. Confirm current GitHub CI and
   security checks when access is available. Do not run live-provider tests
   without intentional prerequisites. Complete when evidence is current for the
   target commit.
5. Draft concise user-facing release notes. Put upgrade and migration actions
   before internal implementation detail. Credit contributors through GitHub's
   generated notes rather than maintaining a manual ledger.
6. Report go/no-go. A release is `go` only when required checks pass, no known
   blocker remains, and rollback/upgrade implications are explicit.

## Output

Return the target, commit range, readiness checklist, validation results,
compatibility/migration notes, release-note draft, and blockers. Separate
verified facts from recommendations.
