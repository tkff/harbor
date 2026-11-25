import { open } from "@tauri-apps/plugin-dialog";
import { Folder, ShieldCheck } from "lucide-react";
import { errorText, type Profile, type IdentityInfo } from "./api";
export type AuthDraft = {
  method: Profile["authMethod"];
  replaceKey: boolean;
  keyPath: string;
  passphrase: string;
  replaceCertificate: boolean;
  certificatePath: string;
};
export function newAuthDraft(profile: Profile | null): AuthDraft {
  return {
    method: profile?.authMethod ?? "password",
    replaceKey: !profile || profile.authMethod === "password",
    keyPath: "",
    passphrase: "",
    replaceCertificate: profile?.authMethod !== "certificate",
    certificatePath: "",
  };
}
export function authenticationUpdate(draft: AuthDraft) {
  return {
    method: draft.method,
    privateKeyPath:
      draft.method !== "password" && draft.keyPath ? draft.keyPath : null,
    passphrase:
      draft.method !== "password" && draft.keyPath ? draft.passphrase : null,
    certificatePath:
      draft.method === "certificate" && draft.certificatePath
        ? draft.certificatePath
        : null,
  };
}
export function CertificateDetails({ identity }: { identity: IdentityInfo }) {
  const now = Date.now() / 1000;
  const expired = identity.validBefore !== null && identity.validBefore <= now;
  const future = identity.validAfter !== null && identity.validAfter > now;
  const date = (time: number | null) =>
    time === null
      ? "—"
      : time > 253402300799
        ? "No expiry"
        : new Date(time * 1000).toLocaleString();
  return (
    <div className="identity-details">
      <div>
        <ShieldCheck size={14} />
        <strong>
          {identity.certificateId === null
            ? "Saved SSH identity"
            : identity.certificateId || "SSH user certificate"}
        </strong>
        {identity.certificateId !== null && (
          <span
            className={expired || future ? "identity-warning" : "secure-label"}
          >
            {expired
              ? "Expired"
              : future
                ? "Not valid yet"
                : "Within validity dates"}
          </span>
        )}
      </div>
      <label>PUBLIC KEY FINGERPRINT</label>
      <code>{identity.fingerprint}</code>
      {identity.certificateId !== null && (
        <>
          <label>PRINCIPALS</label>
          <p>
            {identity.principals.join(", ") ||
              "Any principal, subject to server policy"}
          </p>
          <label>VALIDITY</label>
          <p>
            {date(identity.validAfter)} → {date(identity.validBefore)}
          </p>
          <label>CERTIFICATE AUTHORITY</label>
          <code>{identity.caFingerprint}</code>
        </>
      )}
    </div>
  );
}
export default function AuthenticationFields({
  value,
  onChange,
  profile,
  onError,
}: {
  value: AuthDraft;
  onChange: (draft: AuthDraft) => void;
  profile: Profile | null;
  onError: (error: string) => void;
}) {
  const set = (patch: Partial<AuthDraft>) => onChange({ ...value, ...patch });
  const hasKey = !!profile && profile.authMethod !== "password";
  const hasCert = profile?.authMethod === "certificate";
  const needsKey = !hasKey || value.replaceKey;
  const needsCert = !hasCert || value.replaceCertificate;
  const choose = async (kind: "key" | "certificate") => {
    try {
      const path = await open({
        multiple: false,
        title:
          kind === "key"
            ? "Import SSH private key"
            : "Import OpenSSH user certificate",
        filters: [
          {
            name:
              kind === "key"
                ? "Private key (OpenSSH / PEM / PPK)"
                : "OpenSSH certificate",
            extensions: kind === "key" ? ["*"] : ["pub", "*"],
          },
        ],
      });
      if (typeof path === "string")
        set(kind === "key" ? { keyPath: path } : { certificatePath: path });
    } catch (e) {
      onError(errorText(e));
    }
  };
  return (
    <div className="authentication-fields">
      <label htmlFor="authentication-method">Authentication method</label>
      <select
        id="authentication-method"
        value={value.method}
        onChange={(e) => set({ method: e.target.value as AuthDraft["method"] })}
      >
        <option value="password">Password</option>
        <option value="privateKey">SSH private key</option>
        <option value="certificate">OpenSSH certificate + private key</option>
      </select>
      {value.method !== "password" && (
        <>
          {hasKey && (
            <label className="checkbox-label">
              <input
                type="checkbox"
                checked={value.replaceKey}
                onChange={(e) =>
                  set({
                    replaceKey: e.target.checked,
                    keyPath: "",
                    passphrase: "",
                  })
                }
              />
              Replace saved private key
            </label>
          )}
          {needsKey && (
            <>
              <label htmlFor="ssh-key-path">Private key file</label>
              <div className="path-picker">
                <input
                  id="ssh-key-path"
                  value={value.keyPath}
                  onChange={(e) => set({ keyPath: e.target.value })}
                  placeholder="Choose id_ed25519, a .pem, or .ppk file"
                  required
                />
                <button
                  type="button"
                  className="secondary"
                  onClick={() => void choose("key")}
                >
                  <Folder size={15} />
                  Browse
                </button>
              </div>
              <label htmlFor="ssh-key-passphrase">
                Key passphrase (if encrypted)
              </label>
              <input
                id="ssh-key-passphrase"
                type="password"
                autoComplete="new-password"
                value={value.passphrase}
                onChange={(e) => set({ passphrase: e.target.value })}
              />
            </>
          )}
          {value.method === "certificate" && (
            <>
              {hasCert && (
                <label className="checkbox-label">
                  <input
                    type="checkbox"
                    checked={value.replaceCertificate}
                    onChange={(e) =>
                      set({
                        replaceCertificate: e.target.checked,
                        certificatePath: "",
                      })
                    }
                  />
                  Replace / renew saved certificate
                </label>
              )}
              {needsCert && (
                <>
                  <label htmlFor="ssh-certificate-path">
                    OpenSSH user certificate
                  </label>
                  <div className="path-picker">
                    <input
                      id="ssh-certificate-path"
                      value={value.certificatePath}
                      onChange={(e) => set({ certificatePath: e.target.value })}
                      placeholder="e.g. id_ed25519-cert.pub"
                      required
                    />
                    <button
                      type="button"
                      className="secondary"
                      onClick={() => void choose("certificate")}
                    >
                      <Folder size={15} />
                      Browse
                    </button>
                  </div>
                </>
              )}
            </>
          )}
          <p className="field-hint">
            The imported key, certificate, and passphrase are encrypted inside
            KDBX. The original files are not changed. Server authorization still
            applies.
          </p>
          {profile?.identity && (
            <CertificateDetails identity={profile.identity} />
          )}
        </>
      )}
    </div>
  );
}
