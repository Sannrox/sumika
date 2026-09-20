# Security Policy

## Supported versions

Security fixes target current `main`. There is no released 1.x line yet.
Untagged snapshots and forks do not receive separate security support.

| Version | Supported |
| --- | --- |
| Current `main` | Yes |
| Untagged commits and forks | No |

## Reporting a vulnerability

Please report security vulnerabilities **privately** via GitHub's private
vulnerability reporting: open the
[Security tab](https://github.com/Sannrox/sumika/security/advisories/new)
and click **"Report a vulnerability"**. This keeps the report confidential
until a fix is available.

Do not open public issues or pull requests for exploitable vulnerabilities.

When reporting, include:

- affected commit or version
- steps to reproduce
- expected impact
- whether local sessions, the Unix socket, or process credentials are involved

Do not include real credentials or unredacted sensitive data in a report.

Maintainers will use the private advisory to coordinate reproduction, impact
assessment, remediation, and disclosure.

## Security-relevant behavior

Operators should understand:

- **Local, same-uid socket.** The daemon listens on a user-owned Unix socket
  and accepts only same-uid peers. It is not a network service. Do not place
  the socket on a shared `/tmp` path.
- **The child is a real process.** `start` runs `argv` in a PTY owned by the
  daemon. Treat session names and argv the way you treat any local process
  launcher. Sumika does not sandbox the child.
- **Attach is exclusive.** A second attach steals. The displaced client loses
  the byte stream; the child keeps running.
- **No auto-respawn.** A dead session stays dead until `start`.
- **Secrets.** Do not commit session logs, socket paths with secrets, or
  environment dumps that contain tokens.

## Threat model

### Assets

- The operator's local processes and PTYs
- Anything the child process can read (files, env, credentials)
- Integrity of attach exclusivity (who is typing into the session)

### Adversaries / abuse cases

- Another local uid attempting to connect to the socket
- A stolen attach used to inject input into an existing agent session
- Operator argv that launches a privileged or networked tool without intending to

### Controls today

- Socket parent is user-owned `0700`
- Connect path rejects sockets not owned by the current user
- One focused client; steal disconnects the previous attach

### Non-goals (Phase 0)

- Multi-user or multi-tenant isolation
- OS sandboxing of the child (containers, seatbelt, seccomp)
- Protecting against a fully compromised host process
- Network exposure of the control socket
