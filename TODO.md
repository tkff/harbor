# Harbor roadmap

## FIDO2 / OpenSSH security-key authentication

- [ ] Support OpenSSH FIDO2 security-key identities alongside imported private keys and certificates.
- [ ] Choose and implement cross-platform signing support, including SSH-agent integration where available.
- [ ] Handle hardware-key discovery, touch/user-presence requests, PIN prompts, cancellation, and disconnected tokens.
- [ ] Store only portable identity references and credential metadata in KDBX; hardware private keys remain on the authenticator.
- [ ] Check Windows, macOS, and Linux support and device/provider limitations, including platform authenticator support where feasible.
- [ ] Test successful authentication, wrong PIN, missing key, canceled prompts, and server rejection.
- [ ] Keep website passkeys separate from SSH security-key support; document exactly which authenticators and SSH identities are supported.

## SSH ID-inspired identity management (deferred)

For future consideration only. The user explicitly requested no implementation; do not begin this work without a new request.

- Consider reusable identities assigned to multiple hosts, device-bound SSH keys, and platform biometric approval separate from vault unlocking.
- Keep portable metadata and public keys in KDBX; device-bound private keys stay on their device or authenticator. Plan enrollment and recovery across devices.
- Consider optional public-key bundle export or publishing to user-controlled storage, without requiring Termius or a Harbor account.
- Public-key retrieval alone does not enable authentication. Removing a published key does not revoke keys already installed on servers; plan explicit server-key removal.
- Reference: https://docs.termius.com/ssh-id-passkeys-for-ssh/what-is-ssh-id

## Telnet connections

- [ ] Add Telnet as a connection type with saved hostname and port settings.
- [ ] Implement Telnet option negotiation and interactive terminal input/output.
- [ ] Clearly identify unencrypted Telnet sessions in the connection editor and terminal.
- [ ] Support the existing connection history and optional encrypted terminal recordings.

## Serial connections

- [ ] Add serial-port discovery and connections on Windows, macOS, and Linux.
- [ ] Save port, baud rate, data bits, parity, stop bits, and flow-control settings.
- [ ] Support interactive terminal input/output, line-ending selection, and reconnecting disconnected devices.
- [ ] Support the existing connection history and optional encrypted terminal recordings.

## Dynamic SOCKS forwarding (implemented in v0.11.0)

- [x] Save SOCKS5 proxy rules alongside local and remote forwarding rules.
- [x] Support TCP CONNECT with IPv4, IPv6, and remote hostname resolution.
- [x] Start/stop proxies per SSH session, including automatic startup and cancellation on close/lock.
- [x] Restrict unauthenticated proxies to loopback addresses; bound connections and negotiation time.
- [x] Test negotiation, rejected requests, binary transfers, remote DNS, and cleanup.
- SOCKS4, proxy passwords, BIND, and UDP remain unsupported.

## Other planned work

See [SETTINGS_PLAN.md](SETTINGS_PLAN.md) for remaining preference, locking, log retention/rotation, and sync work. SFTP and split panes are also planned.

Local PTY terminals are implemented. Planned improvements: custom shell selection, local snippets, and encrypted local session recording.
