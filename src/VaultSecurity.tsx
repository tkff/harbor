import { useEffect, useState, type FormEvent } from "react";
import { open, save } from "@tauri-apps/plugin-dialog";
import { Fingerprint, Folder, X, ShieldCheck } from "lucide-react";
import { invoke, errorText, type VaultStatus, type HelloStatus } from "./api";
export default function VaultSecurity({
  status,
  onDone,
  onClose,
}: {
  status: VaultStatus;
  onDone: (status: VaultStatus) => void;
  onClose: () => void;
}) {
  const [hello, setHello] = useState<HelloStatus | null>(null);
  const [password, setPassword] = useState("");
  const [recovery, setRecovery] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const [success, setSuccess] = useState(false);
  useEffect(() => {
    void invoke<HelloStatus>("hello_status", { path: status.vaultPath })
      .then(setHello)
      .catch((e) => setError(errorText(e)));
  }, [status.vaultPath]);
  const choose = async () => {
    try {
      const options = {
        title: status.requiresKeyFile
          ? "Choose the current recovery key file"
          : "Save the recovery key file",
        filters: [
          { name: "KeePass recovery key", extensions: ["key", "keyx"] },
        ],
      };
      const path = status.requiresKeyFile
        ? await open({ ...options, multiple: false })
        : await save({ ...options, defaultPath: "Harbor-recovery.key" });
      if (typeof path === "string") setRecovery(path);
    } catch (e) {
      setError(errorText(e));
    }
  };
  const submit = async (e: FormEvent) => {
    e.preventDefault();
    setError("");
    setBusy(true);
    try {
      const result = await invoke<VaultStatus>("hello_enroll", {
        path: status.vaultPath,
        password,
        recoveryPath: recovery,
      });
      setPassword("");
      setSuccess(true);
      setHello((prev) => (prev ? { ...prev, enabled: true } : prev));
      onDone(result);
    } catch (e) {
      setError(errorText(e));
    } finally {
      setBusy(false);
    }
  };
  return (
    <div className="modal-backdrop">
      <form
        className="modal"
        role="dialog"
        aria-modal="true"
        aria-labelledby="vault-security-title"
        onSubmit={(e) => void submit(e)}
      >
        <button
          type="button"
          className="modal-close"
          aria-label="Close vault security"
          disabled={busy}
          onClick={onClose}
        >
          <X size={19} />
        </button>
        <div className="modal-icon">
          <Fingerprint size={26} />
        </div>
        <div className="eyebrow">VAULT SECOND FACTOR</div>
        <h2 id="vault-security-title">Windows Hello + password.</h2>
        <p>
          Require your master password and Windows Hello to unlock this vault.
          Hello protects the key-file factor on this Windows account.
        </p>
        <div className="security-state">
          <ShieldCheck size={16} />
          {status.requiresKeyFile
            ? "This vault already requires a key-file factor."
            : "This vault currently uses its master password."}
        </div>
        {!hello && (
          <p className="field-hint">Checking Windows Hello availability…</p>
        )}
        {hello?.reason && <p className="field-hint">{hello.reason}</p>}
        {hello?.supported && !success && (
          <>
            <p className="field-hint">
              {status.requiresKeyFile
                ? "Choose the existing recovery key file to register or replace Hello on this PC."
                : "Save a new recovery key file before enabling this. Keep it separate from the vault, preferably offline."}{" "}
              On other platforms or in KeePassXC, unlock with your master
              password and this recovery file.
            </p>
            <fieldset disabled={busy}>
              <label htmlFor="security-password">Current master password</label>
              <input
                id="security-password"
                type="password"
                autoComplete="current-password"
                value={password}
                onChange={(e) => setPassword(e.target.value)}
                required
              />
              <label htmlFor="security-recovery">
                {status.requiresKeyFile
                  ? "Existing recovery key file"
                  : "New recovery key file"}
              </label>
              <div className="path-picker">
                <input
                  id="security-recovery"
                  value={recovery}
                  onChange={(e) => setRecovery(e.target.value)}
                  required
                  placeholder="Choose a safe recovery file location"
                />
                <button
                  type="button"
                  className="secondary"
                  onClick={() => void choose()}
                >
                  <Folder size={15} />
                  Browse
                </button>
              </div>
            </fieldset>
            <p className="field-hint">
              Older copies outside Harbor do not gain this protection. Windows
              will ask you to approve setup. Losing both Hello access and the
              recovery file prevents vault access, even with the master
              password.
            </p>
          </>
        )}
        {success && (
          <div className="security-success">
            Second factor registered. Future unlocks require the master password
            plus Windows Hello or the recovery key file.
          </div>
        )}
        {error && (
          <div className="form-error" role="alert">
            {error}
          </div>
        )}
        <div className="actions">
          <button
            type="button"
            className="secondary"
            disabled={busy}
            onClick={onClose}
          >
            {success ? "Done" : "Close"}
          </button>
          {hello?.supported && !success && (
            <button className="primary" disabled={busy}>
              {busy
                ? "Complete Windows Hello…"
                : status.requiresKeyFile
                  ? "Register Hello on this PC"
                  : "Enable second factor"}
            </button>
          )}
        </div>
      </form>
    </div>
  );
}
