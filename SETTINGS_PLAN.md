# User settings plan

Status: proposed design, not implemented. Session logging and Known hosts listed below are already available in v0.6.0; the broader settings screen remains planned.

## Settings structure

Use a sidebar Settings page with Appearance, Terminal, Connections, Logs, Security, and Storage sections. Show which changes apply immediately and which apply to new sessions. Include Restore defaults per section. Keep global defaults separate from per-host overrides, and show inherited values clearly.

| Section | Proposed options | Proposed starting defaults |
| --- | --- | --- |
| Appearance | System/light/dark theme, accent, compact/comfortable density, sidebar width | System theme, teal accent, comfortable density |
| Terminal | Font family/size, line height, color scheme, cursor shape/blink, scrollback, bell, copy on select, paste behavior | Platform monospace, 14 px, dark terminal, blinking bar, 5,000 lines, bell off, copy on select off |
| Connections | Default SSH port, keepalive, connection timeout, reconnect behavior, default group | Port 22, keepalive 30 seconds, manual reconnect; show these as defaults for new hosts |
| Logs | Recording per host, storage location, retention period, storage budget, text/replay export | Recording off; propose 30-day retention and a user-set storage budget before enabling automatic cleanup |
| Security | Idle auto-lock, lock on OS lock/sleep, close sessions when locking, fingerprint verification, Windows Hello enrollment | Propose 15-minute idle lock and lock on OS lock/sleep; retain mandatory host fingerprint checks |
| Storage | Vault location, local data location, encrypted backups, conflict recovery, optional cloud provider | Local files first; cloud sync stays explicit and user-owned |
| Accessibility | Keyboard shortcuts, reduced motion, focus visibility, text scaling | Follow OS reduced motion and text preferences where supported |

## Current logging behavior

- Host logging is an explicit boolean, off by default; changes affect new sessions.
- AES-256-GCM encrypted records live in the app data directory's `session-logs` folder. The matching KDBX vault holds the protected encryption key. Keys are never saved in application preferences.
- Each encrypted file has a 32 MiB hard limit. Recording errors or the limit disconnect the session with a visible reason, so recording does not silently stop.
- The current log viewer supports search, text preview, refresh, and explicit deletion. Close sessions before deleting logs. There is no automatic retention cleanup yet.
- Plan rotation and retention together: rotate before reaching the limit, keep explicit completion markers, skip active files, and let users review cleanup policy before enabling it.
- Plaintext export must be an explicit action with a clear destination. Cloud log sync should be separate from vault sync and off by default.

## Storage and portability

Store non-secret application preferences in a versioned local JSON file in app data, with validated values and atomic writes. Keep the session log key and portable host overrides inside KDBX. Keep device trust fingerprints in the existing local SQLite store. Device-specific paths, window layout, and fonts should stay local unless the user explicitly enables settings sync. Never store credentials in preferences or diagnostic logs.

## Implementation order

1. Settings shell, validated preference schema, appearance and terminal controls with live previews.
2. Keyboard shortcuts and accessible navigation; reset controls with unsaved-change handling.
3. Idle/OS locking with tests covering active connections, dialogs, and pending authentication.
4. Log rotation, retention and storage-budget controls, with previewable cleanup and explicit export.
5. Optional user-owned cloud sync with conflict handling; separate switches for vault, settings, and logs.

## Known hosts scope

The current Known hosts view shows all host/port/fingerprint pins trusted by Harbor on this device, across vaults. It does not read or edit the system OpenSSH `known_hosts` file. Future import and trust-reset controls need fingerprint comparison, host matching (including non-default ports), and an explicit explanation of the effect before replacing a pin. Routine settings resets must never clear trust pins or weaken fingerprint validation.

Credential recovery is now available after a rejected SSH login: users can retry with temporary credentials or edit the saved host. Future settings may control whether to offer this prompt automatically; retries must continue to validate host fingerprints and must never silently persist credentials.
