import { useEffect, useState, type FormEvent } from "react";
import { X, Check } from "lucide-react";
import { invoke, errorText } from "./api";
import {
  defaultPreferences,
  samePreferences,
  terminalOptions,
  type Preferences,
} from "./preferences";
type Section = "appearance" | "terminal" | "connections" | "logs" | "storage";
const sections: [Section, string][] = [
  ["appearance", "Appearance"],
  ["terminal", "Terminal"],
  ["connections", "Connections"],
  ["logs", "Logs"],
  ["storage", "Storage & security"],
];
export default function Settings({
  preferences,
  ready,
  loadError,
  onSaved,
  onClose,
  onSecurity,
  onLogs,
  onKnownHosts,
}: {
  preferences: Preferences;
  ready: boolean;
  loadError: string;
  onSaved: (p: Preferences) => void;
  onClose: () => void;
  onSecurity: () => void;
  onLogs: () => void;
  onKnownHosts: () => void;
}) {
  const [draft, setDraft] = useState(preferences);
  const [section, setSection] = useState<Section>("appearance");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const [saved, setSaved] = useState(false);
  const [discard, setDiscard] = useState(false);
  const [locations, setLocations] = useState<Record<string, string>>({});
  useEffect(() => setDraft(preferences), [preferences]);
  useEffect(() => {
    let alive = true;
    invoke<Record<string, string>>("app_locations")
      .then((paths) => {
        if (alive) setLocations(paths);
      })
      .catch((e) => {
        if (alive) setError(errorText(e));
      });
    return () => {
      alive = false;
    };
  }, []);
  const dirty = !samePreferences(draft, preferences);
  function change<K extends keyof Preferences>(key: K, value: Preferences[K]) {
    setDraft((p) => ({ ...p, [key]: value }));
    setSaved(false);
    setDiscard(false);
  }
  function close() {
    if (dirty) setDiscard(true);
    else onClose();
  }
  async function save(event: FormEvent) {
    event.preventDefault();
    setBusy(true);
    setError("");
    try {
      const result = await invoke<Preferences>("save_settings", {
        preferences: draft,
      });
      onSaved(result);
      setDraft(result);
      setSaved(true);
      setDiscard(false);
    } catch (e) {
      setError(errorText(e));
    } finally {
      setBusy(false);
    }
  }
  function reset() {
    const keys: Record<Exclude<Section, "storage">, (keyof Preferences)[]> = {
      appearance: ["theme", "accent", "density", "reducedMotion"],
      terminal: [
        "fontFamily",
        "fontSize",
        "lineHeight",
        "terminalTheme",
        "cursorStyle",
        "cursorBlink",
        "scrollback",
      ],
      connections: ["defaultPort", "defaultGroup"],
      logs: ["connectionLogging", "sessionLogging"],
    };
    if (section !== "storage") {
      setDraft((p) => ({
        ...p,
        ...Object.fromEntries(
          keys[section].map((key) => [key, defaultPreferences[key]]),
        ),
      }));
      setSaved(false);
    }
  }
  const preview = terminalOptions(draft);
  return (
    <div className="modal-backdrop">
      <form
        className="modal settings-modal"
        role="dialog"
        aria-modal="true"
        aria-labelledby="settings-title"
        onSubmit={(event) => void save(event)}
      >
        <button
          type="button"
          className="modal-close"
          aria-label="Close settings"
          disabled={busy}
          onClick={close}
        >
          <X size={19} />
        </button>
        <h2 id="settings-title">Settings.</h2>
        <p>
          Personal preferences for this device. App and terminal themes are
          independent.
        </p>
        <div
          role="tablist"
          aria-label="Settings sections"
          className="settings-tabs"
        >
          {sections.map(([id, label]) => (
            <button
              type="button"
              role="tab"
              id={`settings-tab-${id}`}
              aria-controls="settings-panel"
              aria-selected={section === id}
              key={id}
              onClick={() => setSection(id)}
            >
              {label}
            </button>
          ))}
        </div>
        {!ready && <p role="status">Loading preferences...</p>}
        {loadError && (
          <div className="form-error" role="alert">
            {loadError}. Review the defaults and save to replace the local
            preferences file.
          </div>
        )}
        {error && (
          <div className="form-error" role="alert">
            {error}
          </div>
        )}
        <fieldset
          disabled={busy || !ready}
          id="settings-panel"
          role="tabpanel"
          aria-labelledby={`settings-tab-${section}`}
        >
          {section === "appearance" && (
            <>
              <h3>App appearance</h3>
              <label htmlFor="app-theme">App theme</label>
              <select
                id="app-theme"
                value={draft.theme}
                onChange={(e) =>
                  change("theme", e.target.value as Preferences["theme"])
                }
              >
                <option value="system">Follow system</option>
                <option value="light">Light</option>
                <option value="dark">Dark</option>
              </select>
              <p className="field-hint">
                Changes app panels and dialogs. The terminal uses its own color
                scheme.
              </p>
              <div className="form-grid">
                <div>
                  <label htmlFor="app-accent">Accent color</label>
                  <select
                    id="app-accent"
                    value={draft.accent}
                    onChange={(e) =>
                      change("accent", e.target.value as Preferences["accent"])
                    }
                  >
                    <option value="teal">Teal</option>
                    <option value="blue">Blue</option>
                    <option value="violet">Violet</option>
                  </select>
                </div>
                <div>
                  <label htmlFor="app-density">Connection list density</label>
                  <select
                    id="app-density"
                    value={draft.density}
                    onChange={(e) =>
                      change(
                        "density",
                        e.target.value as Preferences["density"],
                      )
                    }
                  >
                    <option value="comfortable">Comfortable</option>
                    <option value="compact">Compact</option>
                  </select>
                </div>
              </div>
              <label className="checkbox-label">
                <input
                  type="checkbox"
                  checked={draft.reducedMotion}
                  onChange={(e) => change("reducedMotion", e.target.checked)}
                />
                Reduce motion
              </label>
              <p className="field-hint">
                The operating system's reduced motion preference is also
                respected.
              </p>
            </>
          )}
          {section === "terminal" && (
            <>
              <h3>Terminal appearance</h3>
              <label htmlFor="terminal-theme">Terminal color scheme</label>
              <select
                id="terminal-theme"
                value={draft.terminalTheme}
                onChange={(e) =>
                  change(
                    "terminalTheme",
                    e.target.value as Preferences["terminalTheme"],
                  )
                }
              >
                <option value="harbor">Harbor dark</option>
                <option value="light">Paper light</option>
                <option value="contrast">High contrast</option>
              </select>
              <p className="field-hint">
                Independent of the app theme. Changes apply to open terminals
                without reconnecting.
              </p>
              <label htmlFor="terminal-font">Font family</label>
              <select
                id="terminal-font"
                value={draft.fontFamily}
                onChange={(e) =>
                  change(
                    "fontFamily",
                    e.target.value as Preferences["fontFamily"],
                  )
                }
              >
                <option value="system">Platform monospace</option>
                <option value="cascadia">Cascadia Code</option>
                <option value="consolas">Consolas</option>
                <option value="jetbrains">JetBrains Mono</option>
              </select>
              <p className="field-hint">
                Fonts must be installed locally; a monospace fallback is used
                otherwise.
              </p>
              <div className="form-grid">
                <div>
                  <label htmlFor="terminal-size">Font size</label>
                  <input
                    id="terminal-size"
                    type="number"
                    min={10}
                    max={28}
                    required
                    value={draft.fontSize}
                    onChange={(e) => change("fontSize", Number(e.target.value))}
                  />
                </div>
                <div>
                  <label htmlFor="terminal-height">Line height</label>
                  <input
                    id="terminal-height"
                    type="number"
                    min={1}
                    max={2}
                    step={0.05}
                    required
                    value={draft.lineHeight}
                    onChange={(e) =>
                      change("lineHeight", Number(e.target.value))
                    }
                  />
                </div>
              </div>
              <div className="form-grid">
                <div>
                  <label htmlFor="terminal-cursor">Cursor shape</label>
                  <select
                    id="terminal-cursor"
                    value={draft.cursorStyle}
                    onChange={(e) =>
                      change(
                        "cursorStyle",
                        e.target.value as Preferences["cursorStyle"],
                      )
                    }
                  >
                    <option value="bar">Bar</option>
                    <option value="block">Block</option>
                    <option value="underline">Underline</option>
                  </select>
                </div>
                <div>
                  <label htmlFor="terminal-scrollback">Scrollback lines</label>
                  <input
                    id="terminal-scrollback"
                    type="number"
                    min={1000}
                    max={50000}
                    step={1000}
                    required
                    value={draft.scrollback}
                    onChange={(e) =>
                      change("scrollback", Number(e.target.value))
                    }
                  />
                </div>
              </div>
              <label className="checkbox-label">
                <input
                  type="checkbox"
                  checked={draft.cursorBlink}
                  onChange={(e) => change("cursorBlink", e.target.checked)}
                />
                Blink cursor
              </label>
              <p className="field-hint">
                Reducing scrollback removes older lines from open terminal
                buffers.
              </p>
              <pre
                aria-label="Terminal theme preview"
                className="settings-terminal-preview"
                style={{
                  background: preview.theme?.background,
                  color: preview.theme?.foreground,
                  fontFamily: preview.fontFamily,
                  fontSize: draft.fontSize,
                  lineHeight: draft.lineHeight,
                }}
              >
                <span style={{ color: preview.theme?.green }}>user@harbor</span>
                {":~$ "}
                <span>ls -la</span>
                {"\nprojects  notes.txt\n$ "}
                <span style={{ color: preview.theme?.cursor }}>
                  {draft.cursorStyle === "block"
                    ? "\u2588"
                    : draft.cursorStyle === "underline"
                      ? "_"
                      : "|"}
                </span>
              </pre>
            </>
          )}
          {section === "connections" && (
            <>
              <h3>Defaults for new hosts</h3>
              <label htmlFor="default-port">Default SSH port</label>
              <input
                id="default-port"
                type="number"
                min={1}
                max={65535}
                required
                value={draft.defaultPort}
                onChange={(e) => change("defaultPort", Number(e.target.value))}
              />
              <label htmlFor="default-group">Default group</label>
              <input
                id="default-group"
                required
                maxLength={80}
                value={draft.defaultGroup}
                onChange={(e) => change("defaultGroup", e.target.value)}
              />
              <p className="field-hint">
                Used when adding a new host. Existing saved connections keep
                their own values. Session logging remains an explicit per-host
                choice and defaults to off.
              </p>
            </>
          )}
          {section === "logs" && (
            <>
              <h3>Logging</h3>
              <label className="checkbox-label">
                <input
                  type="checkbox"
                  checked={draft.connectionLogging}
                  onChange={(e) =>
                    change("connectionLogging", e.target.checked)
                  }
                />
                Keep encrypted connection history
              </label>
              <p className="field-hint">
                On by default. Records attempts, authentication results, and
                disconnect reasons, without terminal output.
              </p>
              <label className="checkbox-label">
                <input
                  type="checkbox"
                  checked={draft.sessionLogging}
                  onChange={(e) => change("sessionLogging", e.target.checked)}
                />
                Record session logs for new hosts by default
              </label>
              <p className="field-hint">
                Off by default. Sets the initial session-log checkbox for new
                hosts. Each host saves its own recording choice. Older hosts
                that used the global default keep that behaviour until you save
                their checkbox. Terminal output may include echoed secrets.
                Connection history is controlled only here and applies to every
                SSH host.
              </p>
            </>
          )}
          {section === "storage" && (
            <>
              <h3>Local storage</h3>
              {[
                ["settings", "Preferences"],
                ["index", "Connection index and trust pins"],
                ["logs", "Encrypted session logs"],
              ].map(([key, label]) => (
                <div className="settings-location" key={key}>
                  <strong>{label}</strong>
                  <code>{locations[key] ?? "Loading location..."}</code>
                </div>
              ))}
              <p className="field-hint">
                Preferences stay on this device. Credentials and the log
                encryption key stay inside the KDBX vault. Encrypted logs are
                separate files.
              </p>
              <div className="settings-links">
                <button
                  type="button"
                  className="secondary"
                  disabled={dirty}
                  onClick={() => {
                    onClose();
                    onSecurity();
                  }}
                >
                  Vault security
                </button>
                <button
                  type="button"
                  className="secondary"
                  disabled={dirty}
                  onClick={() => {
                    onClose();
                    onLogs();
                  }}
                >
                  Connection history and logs
                </button>
                <button
                  type="button"
                  className="secondary"
                  disabled={dirty}
                  onClick={() => {
                    onClose();
                    onKnownHosts();
                  }}
                >
                  Known hosts
                </button>
              </div>
              <p className="field-hint">
                Save or discard changes before opening another panel. Automatic
                locking, retention, rotation, and cloud sync are planned; this
                release does not enable them.
              </p>
            </>
          )}
        </fieldset>
        {discard && (
          <div className="settings-discard" role="alert">
            <p>Discard your unsaved settings?</p>
            <button
              type="button"
              className="secondary"
              onClick={() => setDiscard(false)}
            >
              Keep editing
            </button>
            <button type="button" className="secondary" onClick={onClose}>
              Discard changes
            </button>
          </div>
        )}
        <div className="settings-save-status" role="status">
          {saved
            ? "Saved on this device."
            : dirty
              ? "Unsaved changes. Save to apply them."
              : "Your current preferences."}
        </div>
        <div className="actions">
          {section !== "storage" && (
            <button
              type="button"
              className="secondary"
              disabled={busy || !ready}
              onClick={reset}
            >
              Restore section defaults
            </button>
          )}
          <button
            type="button"
            className="secondary"
            disabled={busy}
            onClick={close}
          >
            Close
          </button>
          <button
            className="primary"
            disabled={busy || !ready || (!dirty && !loadError)}
          >
            <Check size={14} />
            {busy ? "Saving..." : "Save settings"}
          </button>
        </div>
      </form>
    </div>
  );
}
