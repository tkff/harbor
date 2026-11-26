# Harbor 0.11.1 alpha

Harbor is an open-source SSH workspace for Windows, macOS, and Linux, with a KeePass-compatible KDBX vault.

- SSH password, private-key, and OpenSSH user-certificate authentication.
- Saved hosts, groups, managed tags, reusable snippets, and host startup commands.
- Local computer terminals and independent app/terminal themes.
- Local, remote, and dynamic SOCKS5 TCP forwarding.
- Encrypted connection history and optional encrypted session recordings.
- Windows Hello vault second factor, fingerprint checks, and credential correction.
- Fixes changing log filters during a pending recording read.

## Downloads

- Windows x64: `.exe` installer or `.msi`.
- macOS: universal `.dmg` for Apple Silicon and Intel.
- Linux x64: `.AppImage` or `.deb`.
- `SHA256SUMS`: SHA-256 checksums for every installer.

Windows packages are unsigned. macOS packages use ad-hoc signing and are not Apple-notarized. Operating systems may require explicit approval to open them. Automatic updating is not included.

SFTP, FIDO2 SSH authentication, Telnet, serial connections, cloud sync, automatic locking, and split panes remain planned. The SSH ID-inspired work is deferred. SOCKS5 supports TCP only. Local terminals are not recorded yet.
