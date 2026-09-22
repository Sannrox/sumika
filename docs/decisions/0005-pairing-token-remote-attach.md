# Pairing-token remote attach

- Status: accepted (transport decided)
- Date: 2026-09-22
- Source: [Issue #61](https://github.com/Sannrox/sumika/issues/61)
  (direction B, T3-analogue pairing tokens)
- Amends: [0001-week0.md](0001-week0.md) (local-socket lock)

Sumika stays a local Unix-socket daemon, same-uid only, by default. Remote
hosts are explicit, opt-in, per-host configuration. This ADR authorizes the
trust model; it authorizes no implementation PR on its own.

## Trust model (T3 analogue)

- Each daemon issues its own scoped pairing grants. There is no account
  system, no relay, and no T3 Connect equivalent: the local client talks to
  the remote daemon directly over TLS.
- Pairing is a one-time `sumika pair` link shown once. The grant delegates
  scopes that can only narrow, never widen. Only the creation response
  carries the raw credential; afterwards the daemon stores hashes, never
  recoverable secrets.
- Long-lived tokens stay out of attach paths: attach uses short-lived
  tickets. Every RPC method declares its required scope.
- Transport trust and environment auth are separate boundaries. A token that
  reaches the daemon is never, by itself, proof of anything except the
  scopes it names.

## Session semantics across hosts

- Attach exclusivity (steal) holds per daemon. A local client may hold
  attach on a remote session; a second attach steals it.
- No shared attach, no synced habitats, no remote process migration.
- `doctor` reports remote reachability per configured host.

## Alternatives considered

- SSH transport with operator keys: rejected for now. Smaller trust story,
  but shelling attach through `ssh` fights the raw-PTY attach path and
  inherits SSH multiplexing questions (#25 named them a non-goal).
- Mutual TLS with operator-run CA: rejected. Heavier onboarding than
  pairing links for the same per-host trust.
- Tailscale identity: noted, not now. Defers trust to an external
  dependency and leaves non-tailnet operators with nothing.
- Sibling product in rusui scope: rejected by the #61 direction call. This
  is operator session control (picker/attach), not machine management.

## Consequences

- Protocol transport beyond the local socket (TLS + pairing-token auth).
- Per-host remote configuration, pairing-grant storage, revocation, and
  credential renewal, each its own sequenced PR.
- The local default is never weakened: no listen address unless an
  operator configures a remote host entry.
- [VISION.md](../../VISION.md) Family is not amended: rusui still owns
  machines; sumika borrows one operator surface across them.

## Sources

- [0001-week0.md](0001-week0.md) local-socket lock
- [Issue #25](https://github.com/Sannrox/sumika/issues/25)
  multi-machine refusal
- [Issue #61](https://github.com/Sannrox/sumika/issues/61) proposal and
  direction-B decision
- T3 Code `docs/user/remote-access.md` and
  `docs/internals/environment-auth.md` (pairing, scoped sessions,
  relay/environment separation), consulted 2026-09-22
