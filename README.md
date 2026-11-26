# Harbor

A local-first desktop SSH workspace built with Tauri 2, React, TypeScript, Rust, russh, and xterm.js. Open source under MIT. Harbor is a working name.

## Version 0.8.2 alpha

- Create or unlock a password-protected KeePass KDBX vault.
- Add, edit, search, organize, and delete saved SSH connections.
- Store usernames and passwords in KDBX, never in the local SQLite index.
- Connect through SSH password, imported private-key, or OpenSSH user-certificate authentication with interactive xterm.js terminals.
- Open multiple SSH or local shell tabs; send keyboard input, paste, and resize the terminal.
- Approve an unknown server's SHA-256 fingerprint before sending credentials; block changed identities.
- Lock the vault and close all sessions and pending connections.
- Save encrypted backups and refuse to overwrite a vault changed by another application.
- Rebuild connection settings from Harbor custom fields when opening a vault on another computer.
- Save local and remote TCP forwarding rules with each connection, optionally start them on connect, and start or stop them from the terminal’s **Port forwarding** panel.
- Stop listeners and existing forwarded connections when stopping a rule, closing its SSH session, or locking the vault.

Dynamic forwarding / SOCKS proxy, cloud synchronization, FIDO / security-key SSH identities, passkeys, SFTP, split panes, automatic idle locking, and shared-credential selection are not implemented yet.

## Try it

On Windows, open the provided `Harbor.exe`. It uses the Microsoft Edge WebView2 runtime, which normally comes with Windows 11. The first build is unsigned.

1. Choose **Create a vault** and a local `.kdbx` filename.
2. Set a master password with at least 12 characters. Harbor does not store this password between runs and cannot recover it.
3. Choose **New connection**. Enter the SSH host, port, username, and authentication method. Import a private key and matching OpenSSH user certificate when applicable; the server must authorize your selected method.
4. Choose **Connect**, compare the displayed server fingerprint with a trusted source, then choose **Trust & connect**.
5. Use **Lock vault & close sessions** when finished. Normal application exit also closes network sessions.

Use a dedicated Harbor vault while this alpha is being evaluated. The Rust `keepass` library labels KDBX writing experimental. Every save is re-opened for verification before replacement, but comprehensive compatibility testing across all KeePass extensions and database configurations is still needed. KDBX 4 vaults using a password, or password plus key file, are supported. Existing unrelated vault entries are preserved and are not shown as connections.

Connection names, host addresses, ports, groups, tags, and forwarding rules are stored unencrypted in the local SQLite index. Usernames, passwords, imported private keys, their passphrases, and user certificates are stored inside KDBX. Terminal scrollback is held in memory while a tab is open. This version has manual locking; it does not automatically lock on idle or operating-system sleep.

## Local terminals

Choose **Local terminal** in the sidebar, or **+** in the tab bar, to open your computer's shell without unlocking a vault. Windows uses PowerShell 7 when available on PATH, otherwise Windows PowerShell. macOS/Linux use the configured `SHELL` (or `/bin/sh`). Shells start in your home directory with normal user permissions.

Local terminals support keyboard input, resizing, terminal themes, and the same text-copy controls as SSH. Closing a tab ends its shell; **Lock vault & close sessions** also closes local terminals. Local sessions are labelled **not recorded**: connection history and encrypted session recording currently apply only to SSH. Custom shell selection, local snippets, and local recording remain planned.

## Settings and personalization

Open **Settings** in the sidebar, including while the vault is locked. **Appearance** controls the app theme (system/light/dark), accent color, connection list density, and reduced motion. **Terminal** has a separate color scheme selector (Harbor dark, Paper light, High contrast), font presets/size, line height, cursor shape/blink, and scrollback. App and terminal themes are independent: a dark app can have a light terminal and vice versa.

Save changes to apply them. Open terminal sessions update without reconnecting; lowering the scrollback limit removes older buffered lines. Fonts must be installed on your computer or a monospace fallback is used. The Terminal tab includes a live draft preview. **Connections** sets the initial SSH port and group for newly created hosts; it does not modify saved connections. Terminal recording defaults to off; connection history defaults to on. Settings > Logs controls both global defaults, with host-level overrides.

Preferences are validated and atomically saved to `settings.json` in the app data folder, outside the vault. **Storage & security** shows local paths and links to existing security, logging, and trust views. Each editable section has a restore-defaults action; defaults are not applied until saved. Unsaved changes prompt before closing. Automatic idle/OS locking, retention cleanup, log rotation, and cloud sync remain planned.

## Correcting a rejected login

When the SSH server rejects authentication, Harbor opens a credential prompt. Correct the username and password, then choose **Retry connection**. For private-key or certificate authentication, correct the username or choose **Edit saved host** to replace the identity. Each retry creates a fresh SSH connection and repeats server fingerprint checks. The retry credentials stay temporary and do not overwrite the vault; use **Edit saved host** to save permanent corrections. Authentication errors can also reflect server authorization policy, not only a mistyped credential. Other connection errors display their reason and offer **Edit saved host**.

## Connection history and global logging defaults

**Connection history** is enabled by default and records each attempt, successful authentication, established shells, failures, and disconnect reasons. It never records terminal output or raw keyboard input. Failed logins and unreachable hosts are recorded too. History is saved in separate encrypted files from terminal recordings, using the same protected vault log key. Opening **Connection history** in the sidebar shows the event timeline; **Session logs**, directly underneath Connection history, shows terminal recordings.

**Settings > Logs** controls global defaults for connection history (on) and terminal recordings (off). Each host has independent **Use global default / On / Off** choices for both. These settings apply to new connections. Existing hosts retain their prior explicit terminal-recording choice; connection history inherits the global default. New hosts inherit both defaults. The protected log key is created in the vault when logging is first needed, so the vault must be writable for this first connection. Keep a current vault backup with that key.

## Encrypted session logs

Set **Terminal session recording** to **On**, or inherit the global recording default, in a host's connection editor. The global recording default is off and changes apply to new sessions. Harbor records remote terminal output, encrypted connection metadata, timestamps, and a close reason. Raw keystrokes and authentication credentials are not recorded directly; commands or secrets echoed by the server can still appear in output. Terminal recording starts after successful SSH authentication and the terminal displays an active recording indicator.

Logs are stored outside the vault under the app data directory's `session-logs` folder. Filenames are random UUIDs and file contents are AES-256-GCM authenticated encrypted records. A protected key entry inside the KDBX vault derives a different key for each file using HKDF-SHA256 and a random salt. Records use unique counters and authenticated file IDs, salt, and sequence numbers. Do not delete the **Harbor session log encryption key** entry from the vault: without it, existing logs cannot be decrypted. Normal password changes retain this entry. A backup made before logging was first enabled does not contain the key.

Open **Session logs** in the sidebar after unlocking the matching vault. Search, read a text preview, refresh, or explicitly delete saved files after closing SSH sessions. The viewer marks files without an authenticated end record as incomplete or still recording. Damaged or partial encrypted records fail validation. A truncated file ending exactly at a record boundary appears incomplete; the format cannot distinguish that from an interrupted recording. Text previews remove terminal escape sequences and limit displayed output to two million characters; full-screen terminal applications may show repeated screen updates.

Each file has a 32 MiB limit. If encrypted logging fails or reaches that limit, the SSH session disconnects and reports the reason instead of silently dropping output. Automatic rotation, retention cleanup, replay, and export are not implemented yet. Copy the encrypted log files as well as the matching vault to back them up or move them to another device. Vault cloud sync does not automatically sync log files. Filesystem sizes and modification times remain visible to the operating system.

## Known hosts and settings

**Known hosts** shows every hostname, port, and trusted SHA-256 SSH fingerprint saved by Harbor on this device, across all vaults. Search by any of those fields. This is a read-only view; changed host keys remain blocked. It is separate from the operating system's OpenSSH `known_hosts` file.

[Settings plan](SETTINGS_PLAN.md) tracks implemented personalization settings and the remaining security, logging/retention, and sync work.

## Command snippets and host startup

Open **Snippets** in the sidebar after unlocking the vault. Create, edit, search, and delete reusable commands with a name, group, and tags. Snippets support multiple lines and up to 64 KiB of commands. Command bodies are protected fields in native KDBX entries (`Harbor.Type=snippet`), never stored in the local SQLite index. Snippets travel with the vault and are hidden when it is locked.

From a connected terminal, choose **Snippets**, select a command, review or edit the commands for this run, and choose **Run in this session**. This sends the commands and Enter to that terminal through its existing input queue. Commands run in the current shell/application context; Harbor does not create a separate shell or track command exit status. Saving a snippet does not execute it. Edits made in the run preview do not modify the saved snippet.

In **New connection** or **Edit connection**, set **Startup snippet** to a saved snippet, or **None** to disable it. The editor previews the selected commands. Each new connection sends that snippet once after server identity approval, authentication, and successful PTY/shell requests, before accepting keyboard input. Reconnects run it again. Startup uses the snippet's saved contents at connection time; changing a snippet affects all hosts using it on their next connection. This does not wait for a shell prompt or for command completion, so startup snippets should use commands that work immediately in the remote interactive shell. Startup snippets are independent of forwarding startup and should not assume tunnels are ready.

References use the snippet's KDBX UUID, so they remain valid when moving the vault to another computer. Remove a snippet from every host's Startup snippet setting before deleting it. Missing or invalid startup snippets block the connection with an error; they are never silently skipped.

## Groups and tags

Each host belongs to one group (General by default) and can have up to 20 tags. Set the group and comma-separated tags in **New connection** or **Edit connection**. Existing folders become groups automatically. Tags are trimmed, lowercased, deduplicated, and limited to 40 characters each.

Select a group in the sidebar, then optionally select a tag to narrow it further. Click the selected tag again or the tag filter's � button to clear it. Search matches host names, addresses, groups, and tags. Tags on host rows are clickable filters.

Groups use the existing portable `Harbor.Folder` field; tags use native KDBX entry tags, visible in KeePass-compatible applications. Both are recovered when opening the vault on another computer. Empty groups and standalone tags are not stored; they disappear from the sidebar when no host uses them.

## SSH private keys and OpenSSH certificates

In **New connection** or **Edit connection**, choose **SSH private key** or **OpenSSH certificate + private key**. Import an OpenSSH, PEM, or supported PPK private-key file and its passphrase if encrypted. Certificate authentication also needs the matching OpenSSH user certificate, commonly named id_ed25519-cert.pub. The files are imported into protected KDBX fields; Harbor does not modify the originals or depend on those file paths after saving.

The editor shows the public-key fingerprint and, for certificates, the certificate ID, principals, CA fingerprint, and validity dates. Use **Replace / renew saved certificate** to import a renewed certificate while keeping the existing private key. Mismatched keys, invalid certificate signatures, and host certificates used as user certificates are rejected. Expired or future-dated certificates can be stored for inspection, but cannot be used to connect. The SSH server decides which public keys, certificate authorities, principals, and certificate options it authorizes. This version does not issue certificates or add SSH host-CA trust management.

Password authentication remains available and does not require an SSH identity. An identity such as id_ed25519 is a private key, not another account. Website passkeys and OpenSSH FIDO/security-key identities are separate from imported keys. FIDO SSH signing and passkey support are future work.

## Windows Hello as a vault second factor

Unlock a dedicated test vault, then choose **Credential vault** in the sidebar (or **About this workspace → Vault security / Windows Hello**). Enter the current master password, choose a NEW recovery key-file location, and enable the second factor. Complete the Windows Hello enrollment prompt yourself. Existing files are never overwritten.

This changes the vault to standard KeePass password-plus-key-file encryption. A random 32-byte key file is saved at your selected recovery location. Windows Hello signs a random challenge with a non-exportable Windows key; a hash of that signature protects the local factor using AES-256-GCM. Only that encrypted factor record, its challenge, and its Windows key identifier are stored in local SQLite. The master password is not cached. Every normal Hello unlock still requires the master password AND approval through Windows Hello (PIN, fingerprint, or face as supported by Windows).

Keep the recovery key file separate from your vault, preferably offline. On macOS/Linux, in KeePassXC, after a Windows Hello reset, or if Hello is unavailable, open with the SAME master password AND recovery key file. The password alone cannot unlock a protected vault. To register Hello on another Windows PC, first unlock using the recovery file, then choose the existing recovery file in Vault security; the vault's factor is preserved. Each PC needs its own local Hello enrollment. The local Hello record is not part of the KDBX file and should not be synced as a portable credential.

Losing BOTH Hello access and the recovery key file prevents access even if you know the master password. Older copies and external backups created before enrollment retain their old protection; manage those separately. Enabling 2FA protects the current vault and Harbor's own backup. This alpha does not yet include an in-app operation to remove the key-file factor; KeePassXC can manage the master-key components using the password and recovery file.

This is vault protection, not Windows Hello SSH authentication and not a website passkey. Microsoft’s KeyCredential APIs are used directly; cancellation, unavailable Hello, missing Windows keys, invalid ciphertext, and incorrect composite vault credentials do not fall back to password-only access. Native Windows Hello enrollment and fingerprint/PIN interactions require manual testing on a Hello-enabled Windows account; automated tests do not claim to validate biometric hardware.

## Port forwarding

Edit or create a connection and choose **Add forwarding rule**. Choose **Local** or **Remote**, enter the listening address/port and destination host/port, and choose whether to start it with the SSH session. The default listen address is 127.0.0.1. Save and reconnect to apply edited rules to a new session.

| Direction | Listener runs on | Destination is reached from | Example |
| --- | --- | --- | --- |
| Local | Your computer | SSH server | Local 127.0.0.1:5433 → server-side 127.0.0.1:5432 |
| Remote | SSH server | Your computer | Server 127.0.0.1:9000 → your local 127.0.0.1:3000 |

Open **Port forwarding** in a terminal tab to see each rule’s actual state and start or stop it. Stopping a rule terminates its existing forwarded TCP connections. A failed listener does not close the terminal. Remote forwarding depends on the SSH server permitting it; binding beyond loopback also depends on the server’s GatewayPorts policy and the network/firewall.

Rules are scoped to an SSH session. Two tabs cannot both bind the same local address/port; one will report a port-in-use error. Rule changes affect new sessions. Up to 32 rules per connection and 128 simultaneous TCP connections per rule are supported. Listen and destination ports must be 1–65535; server-assigned ephemeral remote ports, UDP, and Unix socket forwarding are not included. Listeners default to loopback; deliberately choosing a broader listen address may expose the forwarded service to other computers.

Remote listen/cancel requests have a 10-second deadline. If a server fails to acknowledge one, Harbor closes the SSH session so an unconfirmed server-side listener cannot remain attached to a live session. If a server explicitly refuses cancellation, Harbor keeps the rule marked active with an error; close that SSH session to release it.

Dynamic forwarding / SOCKS proxy remains a future feature. The direction selector and session panel identify it as planned; this version creates no SOCKS listener.

## Development

Install Node.js 22+ and current stable Rust, plus [Tauri's platform prerequisites](https://v2.tauri.app/start/prerequisites/). The included `rust-toolchain.toml` selects stable Rust; current dependencies require Rust 1.89 or later.

```sh
npm ci
npm run desktop
```

`npm run dev` starts a browser preview only. Vault access and SSH require the native desktop application.

Build an installer for the operating system you are using:

```sh
npm run package
```

Windows needs Microsoft C++ Build Tools and WebView2; macOS needs Xcode command-line tools; Linux needs WebKitGTK and GTK development packages. Build and test on each target OS. A GitHub Actions workflow is included for Windows, macOS, and Ubuntu; it has not been executed remotely in this workspace.

## Validation

```sh
npm run build
npm run test:backend
npm run test:interop
cd src-tauri
cargo fmt --check
cargo clippy --all-targets -- -D warnings
```

The backend tests create temporary vaults and a loopback-only SSH server. They exercise encrypted credentials, wrong-password rejection, create-without-overwrite, backups, external-file conflicts, connection recovery from KDBX, host trust, SSH authentication, terminal data, resizing, and disconnect. They do not connect to your servers.

`test:interop` uses a separate JavaScript KDBX implementation (`kdbxweb` with `hash-wasm`) to read a Rust-created file, validate test credentials, edit it with history/custom fields, and save it for Rust to reopen. These libraries are development-only and are not included in the desktop application.

An optional KeePassXC CLI interoperability test is also included. Set `KEEPASSXC_CLI` to its executable path, then run:

```sh
cargo test --manifest-path src-tauri/Cargo.toml keepassxc_interoperability -- --ignored --nocapture
```

Validation of the initial Windows build: frontend production build, Rust formatting and Clippy, the core backend tests, including real local and remote TCP forwarding traffic and lifecycle checks, and the independent KDBX interoperability test passed. The native Windows app launched and exposed its expected interface; the workspace and vault form were visually checked in the browser preview. The installed KeePassXC CLI did not complete, so that optional test was not run successfully. macOS and Linux builds have not been tested here. The full native click-through against a user-owned SSH server remains to be tried.

## Storage and architecture

```text
React UI / xterm.js
       │ local Tauri IPC
       ▼
Rust commands ── russh ── SSH server
       │
       ├─ KDBX: encrypted credentials and portable connection fields
       └─ SQLite: connection index, last vault path, trusted host fingerprints, encrypted local Hello factor records
```

The SQLite file is `harbor.db` in Tauri's platform app-data directory for `dev.harbor.ssh`. You choose the vault's location. Each connection is a KDBX entry; its UUID links the local index to the vault. `Harbor.Type`, `Harbor.Host`, `Harbor.Port`, `Harbor.Folder`, and `Harbor.Forwards`, and `Harbor.AuthMethod` identify portable connection records. Arbitrary existing KeePass entries are not automatically imported as SSH profiles.

New vaults use KDBX 4.1, AES-256 outer encryption, and Argon2id with 64 MiB memory, 3 iterations, and parallelism 2. Credentials are passed to the Rust SSH client; saved passwords are never returned to the interface. Rust password buffers use zeroizing wrappers, although this does not guarantee that every transient copy in the UI, libraries, or operating system is erased.

Writes use a temporary file in the vault directory, synchronization, read-back verification, and atomic file replacement. The prior version is copied to `<name>.kdbx.bak`. When enabling a second factor, both the live vault and Harbor’s backup are written with the new composite key; an unprotected-by-the-second-factor backup is not left behind. A file digest detects external changes before saving. This is single-device conflict protection, not multi-device cloud synchronization or a multi-process file-locking protocol. Avoid simultaneous editing.

Known server keys are pinned per hostname and port. This version blocks changed keys; there is no in-app reset button yet. Private keys, master passwords, terminal output, and cloud tokens are not logged. There are no analytics or hosted application services.

Future cloud sync can transport the encrypted vault directly through OneDrive or Google Drive APIs, with revision checks and explicit conflict handling.

Version 0.3.0 validation adds encrypted-key and OpenSSH-certificate SSH logins against a loopback server, rejection of untrusted CAs, key/certificate matching and validity checks, Hello envelope tamper checks, two-factor vault and backup enforcement, recovery on a fresh local database, and independent KDBX password-plus-key-file interoperability. Native Hello prompts remain a manual test.

Version 0.4.0 validation covers legacy SQLite upgrades, group/tag portability, native KDBX tag reads and edits with the independent kdbxweb library, and tag validation. Browser UI checks cover combined filters, tag search, and editing a host's group and tags.

Version 0.5.0 validation covers encrypted snippet CRUD, portable startup references, deletion protection, control-character rejection, and startup delivery exactly once before user input against a loopback SSH server.

Version 0.6.0 validation covers encrypted log roundtrips, corruption and truncation detection, wrong-key and renamed-file rejection, logging opt-in and key portability, and encrypted output capture in the loopback SSH integration test.

Login retry validation uses a loopback SSH server to reject saved credentials, request corrected credentials, connect successfully with temporary overrides, preserve the original saved credentials, and verify that failed authentication creates no session log.

Version 0.7.0 validation checks preference defaults, partial-file migration, atomic persistence, validation failures preserving the saved file, corrupt-file recovery, and UI theme independence and live terminal changes.

Version 0.7.1 fixes initial terminal fitting so padding cannot hide the last row, schedules a redraw and remote size update after connection/layout/font changes, and records future FIDO2 SSH support in [TODO.md](TODO.md).

Terminal fitting regression checks verified the bottom row before keyboard input, with large fonts/line height, with the forwarding panel open, and with encrypted logging's recording banner visible.

Version 0.8.0 separates encrypted connection history from terminal recordings, adds independent global defaults and per-host inheritance, and validates failed/successful connection history without output capture in the SSH integration test.

Version 0.8.1 adds a prominent connection-failure panel for timeouts, unreachable hosts, refused connections, and other failed setup attempts. It shows the complete error plus Edit host and Retry connection. Retrying after editing uses the latest saved host settings.

The timeout recovery UI was checked through Edit host, changing hostname and port, saving, and retrying successfully with the corrected values. Retried connections retain the active terminal tab.

## Copying terminal text

Select output with the mouse, then use the terminal toolbar's **Copy** button, **Ctrl+C**, **Ctrl+Shift+C**, or **Cmd+C** on macOS. Copy works in connected and closed tabs. With no selection, Ctrl+C retains its normal remote interrupt behavior. In mouse-aware applications such as tmux or Vim, hold Shift while dragging to select text locally. The Copy button is enabled when text is selected and shows success or an error without closing the SSH session. Harbor uses Tauri's native text clipboard API with write-only permission; it does not read or automatically copy clipboard contents. Existing paste behavior is unchanged.

Version 0.8.2 validation covers copy/interrupt shortcut routing, multiline mouse selection followed by Copy and paste into a text field, native clipboard plugin registration and write-only permissions, plus the backend and KDBX compatibility suites.

Version 0.9.0 adds local PTY terminals and nests Session logs under Connection history. Local terminal tests cover command execution, PTY resizing, final output before exit, pending cancellation, and closing a running shell. Windows tests passed; macOS/Linux builds were not run locally for this change.
