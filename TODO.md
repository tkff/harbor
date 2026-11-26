# Harbor roadmap

## FIDO2 / OpenSSH security-key authentication

- [ ] Support OpenSSH FIDO2 security-key identities alongside imported private keys and certificates.
- [ ] Choose and implement cross-platform signing support, including SSH-agent integration where available.
- [ ] Handle hardware-key discovery, touch/user-presence requests, PIN prompts, cancellation, and disconnected tokens.
- [ ] Store only portable identity references and credential metadata in KDBX; hardware private keys remain on the authenticator.
- [ ] Check Windows, macOS, and Linux support and device/provider limitations, including platform authenticator support where feasible.
- [ ] Test successful authentication, wrong PIN, missing key, canceled prompts, and server rejection.
- [ ] Keep website passkeys separate from SSH security-key support; document exactly which authenticators and SSH identities are supported.

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

## Other planned work

See [SETTINGS_PLAN.md](SETTINGS_PLAN.md) for remaining preference, locking, log retention/rotation, and sync work. Dynamic SOCKS forwarding, SFTP, local shells, and split panes are also planned.
