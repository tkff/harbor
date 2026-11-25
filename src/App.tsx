import { useEffect, useMemo, useState, type FormEvent } from "react";
import { open, save } from "@tauri-apps/plugin-dialog";
import {
  Anchor,
  Code2,
  ArrowDownLeft,
  ArrowRight,
  Check,
  ChevronRight,
  CircleHelp,
  FileKey2,
  Folder,
  Tag,
  KeyRound,
  Laptop,
  LockKeyhole,
  MoreHorizontal,
  Plus,
  Search,
  Server,
  ShieldCheck,
  Terminal,
  Trash2,
  UnlockKeyhole,
  X,
} from "lucide-react";
import {
  invoke,
  isTauri,
  errorText,
  type Profile,
  type Session,
  type VaultStatus,
  type HelloStatus,
  type Snippet,
} from "./api";
import TerminalPane from "./TerminalPane";
import ForwardingEditor from "./ForwardingEditor";
import AuthenticationFields, {
  newAuthDraft,
  authenticationUpdate,
} from "./AuthenticationFields";
import VaultSecurity from "./VaultSecurity";
import Snippets from "./Snippets";

const empty: VaultStatus = {
  unlocked: false,
  vaultPath: null,
  profiles: [],
  requiresKeyFile: false,
};
const filename = (path: string | null) =>
  path?.split(/[\\/]/).pop() || "No vault selected";
type VaultMode = "create" | "open" | "unlock";

export default function App() {
  const [status, setStatus] = useState<VaultStatus>(empty);
  const [sessions, setSessions] = useState<Session[]>([]);
  const [activeId, setActiveId] = useState<string | null>(null);
  const [search, setSearch] = useState("");
  const [filter, setFilter] = useState<string | null>(null);
  const [tagFilter, setTagFilter] = useState<string | null>(null);
  const [vaultMode, setVaultMode] = useState<VaultMode | null>(null);
  const [editor, setEditor] = useState<Profile | "new" | null>(null);
  const [deleteTarget, setDeleteTarget] = useState<Profile | null>(null);
  const [error, setError] = useState("");
  const [busy, setBusy] = useState(false);
  const [help, setHelp] = useState(false);
  const [vaultSecurity, setVaultSecurity] = useState(false);
  const [showSnippets, setShowSnippets] = useState(false);
  const native = isTauri();

  useEffect(() => {
    if (native)
      void invoke<VaultStatus>("status")
        .then(setStatus)
        .catch((e) => setError(errorText(e)));
  }, [native]);

  const folders = useMemo(
    () => [...new Set(status.profiles.map((p) => p.folder))].sort(),
    [status.profiles],
  );
  const tags = [...new Set(status.profiles.flatMap((p) => p.tags))].sort();
  const shown = status.profiles.filter(
    (p) =>
      (filter === null || p.folder === filter) &&
      (!tagFilter || p.tags.includes(tagFilter)) &&
      `${p.name} ${p.host} ${p.folder} ${p.tags.join(" ")}`
        .toLowerCase()
        .includes(search.toLowerCase()),
  );
  const connected = sessions.filter((s) => s.state === "connected").length;
  const active = sessions.find((s) => s.id === activeId);

  const closeTab = (id: string) => {
    void invoke("ssh_close", { sessionId: id }).catch(() => {});
    setSessions((prev) => {
      const next = prev.filter((s) => s.id !== id);
      if (activeId === id) setActiveId(next.at(-1)?.id ?? null);
      return next;
    });
  };
  const connect = (profile: Profile) => {
    const session: Session = {
      id: crypto.randomUUID(),
      profile,
      state: "connecting",
    };
    setSessions((prev) => [...prev, session]);
    setActiveId(session.id);
  };
  const lock = async () => {
    try {
      await invoke("lock_vault");
      setSessions([]);
      setActiveId(null);
      setEditor(null);
      setVaultSecurity(false);
      setShowSnippets(false);
      setStatus((prev) => ({ ...prev, unlocked: false, profiles: [] }));
    } catch (e) {
      setError(errorText(e));
    }
  };
  const remove = async () => {
    if (!deleteTarget) return;
    setBusy(true);
    try {
      setStatus(
        await invoke<VaultStatus>("delete_connection", { id: deleteTarget.id }),
      );
      setDeleteTarget(null);
    } catch (e) {
      setError(errorText(e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="app-shell">
      <aside className="sidebar">
        <a
          className="brand"
          href="#"
          onClick={(e) => {
            e.preventDefault();
            setActiveId(null);
          }}
        >
          <span className="brand-mark">
            <Anchor size={23} strokeWidth={1.7} />
          </span>
          <span>
            harbor<span className="brand-period">.</span>
          </span>
          <span className="version">ALPHA</span>
        </a>
        <div className="workspace-label">PERSONAL WORKSPACE</div>
        <button
          className={`nav-item ${!activeId ? "selected" : ""}`}
          onClick={() => {
            setActiveId(null);
            setFilter(null);
            setTagFilter(null);
            setSearch("");
          }}
        >
          <Server size={17} />
          <span>Connections</span>
          <span className="count">{status.profiles.length}</span>
        </button>
        <button
          className="nav-item"
          onClick={() =>
            status.unlocked
              ? setVaultSecurity(true)
              : setVaultMode(status.vaultPath ? "unlock" : "create")
          }
        >
          <ShieldCheck size={17} />
          <span>Credential vault</span>
          <span className={`tiny-dot ${status.unlocked ? "green" : ""}`} />
        </button>
        <button
          className="nav-item"
          disabled={!status.unlocked}
          onClick={() => setShowSnippets(true)}
        >
          <Code2 size={17} />
          <span>Snippets</span>
        </button>
        <div className="host-organization">
          <div className="section-label">
            GROUPS<span>{folders.length}</span>
          </div>
          <button
            className={`folder-item ${filter === null && !activeId ? "current" : ""}`}
            onClick={() => {
              setFilter(null);
              setActiveId(null);
            }}
          >
            <Folder size={15} />
            <span>All connections</span>
            <span>{status.profiles.length}</span>
          </button>
          {folders.map((folder) => (
            <button
              key={folder}
              className={`folder-item ${filter === folder && !activeId ? "current" : ""}`}
              onClick={() => {
                setFilter(folder);
                setActiveId(null);
              }}
            >
              <Folder size={15} />
              <span>{folder}</span>
              <span>
                {status.profiles.filter((p) => p.folder === folder).length}
              </span>
            </button>
          ))}
          {!folders.length && (
            <div className="sidebar-hint">
              Organize your servers into groups when you add a connection.
            </div>
          )}
          <div className="section-label">
            TAGS<span>{tags.length}</span>
          </div>
          {tags.map((tag) => (
            <button
              key={tag}
              className={`folder-item ${tagFilter === tag ? "current" : ""}`}
              onClick={() => {
                setTagFilter(tagFilter === tag ? null : tag);
                setActiveId(null);
              }}
            >
              <Tag size={14} />
              <span>{tag}</span>
              <span>
                {status.profiles.filter((p) => p.tags.includes(tag)).length}
              </span>
            </button>
          ))}
          {!tags.length && (
            <div className="sidebar-hint">
              Add tags such as production, linux, or work to your hosts.
            </div>
          )}
        </div>
        <div className="sidebar-bottom">
          <div className={`vault-summary ${status.unlocked ? "is-open" : ""}`}>
            <div className="vault-summary-top">
              <ShieldCheck size={17} />
              <strong>
                {status.unlocked ? "Vault unlocked" : "Vault locked"}
              </strong>
              <span className={`tiny-dot ${status.unlocked ? "green" : ""}`} />
            </div>
            <div className="vault-filename" title={status.vaultPath ?? ""}>
              {filename(status.vaultPath)}
            </div>
            <button
              onClick={() =>
                status.unlocked
                  ? void lock()
                  : setVaultMode(status.vaultPath ? "unlock" : "create")
              }
            >
              {status.unlocked ? (
                <LockKeyhole size={13} />
              ) : (
                <UnlockKeyhole size={13} />
              )}{" "}
              {status.unlocked
                ? "Lock vault & close sessions"
                : status.vaultPath
                  ? "Unlock vault"
                  : "Set up your vault"}
            </button>
          </div>
          <button className="help-link" onClick={() => setHelp(true)}>
            <CircleHelp size={15} />
            About this workspace
            <ArrowDownLeft size={13} />
          </button>
        </div>
      </aside>

      <div className="main-shell">
        <header className="topbar">
          <div className="breadcrumb">
            <span>Workspace</span>
            <ChevronRight size={13} />
            <strong>{active ? active.profile.name : "Connections"}</strong>
          </div>
          <div className="topbar-right">
            <span className="local-badge">
              <span className="tiny-dot green" />
              Local first
            </span>
            <span className="avatar">ME</span>
          </div>
        </header>
        <div className="tabbar">
          <button
            className={`home-tab ${!activeId ? "tab-active" : ""}`}
            onClick={() => setActiveId(null)}
          >
            <Server size={14} />
            Connections
          </button>
          {sessions.map((s) => (
            <div
              key={s.id}
              className={`session-tab ${activeId === s.id ? "tab-active" : ""}`}
            >
              <button onClick={() => setActiveId(s.id)}>
                <span className={`status-dot ${s.state}`} />
                {s.profile.name}
              </button>
              <button
                className="close-tab"
                aria-label={`Close ${s.profile.name}`}
                onClick={() => closeTab(s.id)}
              >
                <X size={12} />
              </button>
            </div>
          ))}
          <button
            className="tab-plus"
            aria-label="View connections"
            onClick={() => setActiveId(null)}
          >
            <Plus size={15} />
          </button>
        </div>
        {error && (
          <div className="error-banner" role="alert">
            {error}
            <button aria-label="Dismiss error" onClick={() => setError("")}>
              <X size={16} />
            </button>
          </div>
        )}
        {!native && (
          <div className="preview-banner">
            Browser preview · Open the desktop app to use the vault and SSH
            connections.
          </div>
        )}

        <main className={`connections-page ${activeId ? "hidden" : ""}`}>
          <div className="page-heading">
            <div>
              <div className="eyebrow">YOUR INFRASTRUCTURE, WITHIN REACH</div>
              <h1>
                Connections<span className="heading-dot">.</span>
              </h1>
              <p>A quiet place for all your servers.</p>
            </div>
            <button
              className="primary"
              disabled={!native}
              onClick={() =>
                status.unlocked
                  ? setEditor("new")
                  : setVaultMode(status.vaultPath ? "unlock" : "create")
              }
            >
              <Plus size={16} />
              New connection
            </button>
          </div>
          <div className="metrics">
            <div className="metric">
              <span className="metric-icon">
                <Server size={18} />
              </span>
              <div>
                <span>Saved connections</span>
                <strong>
                  {status.profiles.length.toString().padStart(2, "0")}
                </strong>
              </div>
            </div>
            <div className="metric">
              <span className="metric-icon teal">
                <Terminal size={18} />
              </span>
              <div>
                <span>Active sessions</span>
                <strong>
                  {connected.toString().padStart(2, "0")}
                  <span className="metric-note">
                    {connected ? "connected" : "ready when you are"}
                  </span>
                </strong>
              </div>
            </div>
            <div className="metric vault-metric">
              <span className="metric-icon">
                <FileKey2 size={19} />
              </span>
              <div>
                <span>Credential storage</span>
                <strong className="metric-text">
                  KDBX vault
                  <span className="secure-label">
                    <ShieldCheck size={11} />
                    Encrypted
                  </span>
                </strong>
              </div>
            </div>
          </div>

          {!status.unlocked ? (
            <section className="onboarding-panel">
              <div className="vault-art">
                <div className="vault-art-ring" />
                <div className="vault-art-box">
                  <LockKeyhole size={38} strokeWidth={1.3} />
                </div>
                <span className="art-check">
                  <ShieldCheck size={17} />
                </span>
              </div>
              <div className="eyebrow">A SECURE START</div>
              <h2>
                {status.vaultPath
                  ? "Welcome back to your workspace."
                  : "Your servers. Your keys. Your control."}
              </h2>
              <p>
                {status.vaultPath
                  ? "Unlock your vault to access saved connections and start a session."
                  : "Create a KeePass-compatible vault to keep your credentials encrypted and your connections close."}
              </p>
              <div className="onboarding-actions">
                <button
                  className="primary"
                  disabled={!native}
                  onClick={() =>
                    setVaultMode(status.vaultPath ? "unlock" : "create")
                  }
                >
                  {status.vaultPath ? (
                    <UnlockKeyhole size={15} />
                  ) : (
                    <Plus size={15} />
                  )}{" "}
                  {status.vaultPath ? "Unlock your vault" : "Create a vault"}
                  <ArrowRight size={15} />
                </button>
                <button
                  className="secondary"
                  disabled={!native}
                  onClick={() => setVaultMode("open")}
                >
                  <Folder size={15} />
                  Open existing vault
                </button>
              </div>
              <div className="onboarding-footnotes">
                <span>
                  <Check size={13} />
                  Stored on your device
                </span>
                <span>
                  <Check size={13} />
                  KeePass compatible
                </span>
                <span>
                  <Check size={13} />
                  No account required
                </span>
              </div>
            </section>
          ) : (
            <>
              <div className="list-toolbar">
                <div className="list-title">
                  <h2>{filter ?? "All connections"}</h2>
                  {tagFilter && (
                    <button
                      className="tag-chip"
                      onClick={() => setTagFilter(null)}
                      aria-label="Clear tag filter"
                    >
                      {tagFilter}
                      <X size={12} />
                    </button>
                  )}
                  <span className="pill">{shown.length}</span>
                </div>
                <div className="search">
                  <Search size={15} />
                  <input
                    aria-label="Search connections"
                    placeholder="Search hosts, groups, tags..."
                    value={search}
                    onChange={(e) => setSearch(e.target.value)}
                  />
                  {search && (
                    <button
                      aria-label="Clear search"
                      onClick={() => setSearch("")}
                    >
                      <X size={13} />
                    </button>
                  )}
                </div>
              </div>
              {shown.length ? (
                <div className="connection-list">
                  <div className="list-header">
                    <span>CONNECTION</span>
                    <span>HOST / PORT</span>
                    <span>GROUP</span>
                    <span />
                  </div>
                  {shown.map((p) => (
                    <div className="connection-row" key={p.id}>
                      <div className="connection-name">
                        <span className="server-tile">
                          <Server size={20} />
                        </span>
                        <div>
                          <strong>{p.name}</strong>
                          <span>
                            SSH ·{" "}
                            {p.authMethod === "certificate"
                              ? "OpenSSH certificate"
                              : p.authMethod === "privateKey"
                                ? "Private key"
                                : "Password"}{" "}
                            authentication
                          </span>
                          <div className="host-tags">
                            {p.tags.map((tag) => (
                              <button
                                key={tag}
                                className="tag-chip"
                                onClick={() => setTagFilter(tag)}
                              >
                                {tag}
                              </button>
                            ))}
                          </div>
                        </div>
                      </div>
                      <div className="host-cell">
                        {p.host}
                        <span>Port {p.port}</span>
                      </div>
                      <div className="folder-cell">
                        <Folder size={13} />
                        {p.folder}
                      </div>
                      <div className="row-actions">
                        <button
                          className="connect-button"
                          onClick={() => connect(p)}
                        >
                          <Terminal size={14} />
                          Connect
                          <ArrowRight size={13} />
                        </button>
                        <button
                          className="icon-button"
                          aria-label={`Edit ${p.name}`}
                          title="Edit connection"
                          onClick={() => setEditor(p)}
                        >
                          <MoreHorizontal size={18} />
                        </button>
                        <button
                          className="icon-button danger-hover"
                          aria-label={`Delete ${p.name}`}
                          title="Delete connection"
                          onClick={() => setDeleteTarget(p)}
                        >
                          <Trash2 size={15} />
                        </button>
                      </div>
                    </div>
                  ))}
                </div>
              ) : (
                <section className="empty-connections">
                  <span className="empty-icon">
                    <Server size={28} strokeWidth={1.3} />
                  </span>
                  <h2>
                    {search || tagFilter || filter !== null
                      ? "No matching connections"
                      : "Make your first connection."}
                  </h2>
                  <p>
                    {search || tagFilter || filter !== null
                      ? "Try another search, group, or tag."
                      : "Add a server. Your credentials go straight into your encrypted vault."}
                  </p>
                  {!search && (
                    <button
                      className="secondary"
                      onClick={() => setEditor("new")}
                    >
                      <Plus size={15} />
                      Add connection
                    </button>
                  )}
                </section>
              )}
            </>
          )}
          <div className="bottom-note">
            <ShieldCheck size={14} />
            <span>
              Credentials stay encrypted in your vault. SSH connects directly to
              your servers.
            </span>
            <span className="protocol-label">SSH-2</span>
          </div>
        </main>
        <div className={`terminal-area ${!activeId ? "hidden" : ""}`}>
          {sessions.map((session) => (
            <TerminalPane
              key={session.id}
              session={session}
              active={session.id === activeId}
              onState={(id, state) =>
                setSessions((prev) =>
                  prev.map((s) => (s.id === id ? { ...s, state } : s)),
                )
              }
            />
          ))}
        </div>
        <footer className="statusbar">
          <span>
            <span className={`tiny-dot ${status.unlocked ? "green" : ""}`} />
            {status.unlocked ? "Vault unlocked" : "Vault locked"}
            <span className="status-separator">|</span>
            {sessions.length} session{sessions.length === 1 ? "" : "s"}
          </span>
          <span>
            <Laptop size={12} />
            On this device<span className="status-separator">|</span>Harbor
            v0.5.0
          </span>
        </footer>
      </div>

      {vaultMode && (
        <VaultModal
          mode={vaultMode}
          previousPath={status.vaultPath}
          onClose={() => setVaultMode(null)}
          onDone={(s) => {
            setStatus(s);
            setVaultMode(null);
            setError("");
          }}
        />
      )}
      {showSnippets && status.unlocked && (
        <Snippets onClose={() => setShowSnippets(false)} />
      )}
      {vaultSecurity && status.unlocked && (
        <VaultSecurity
          status={status}
          onDone={setStatus}
          onClose={() => setVaultSecurity(false)}
        />
      )}
      {editor && (
        <ConnectionModal
          profile={editor === "new" ? null : editor}
          folders={folders}
          onClose={() => setEditor(null)}
          onDone={(s) => {
            setStatus(s);
            setEditor(null);
          }}
        />
      )}
      {deleteTarget && (
        <div className="modal-backdrop">
          <section
            className="modal small-modal"
            role="dialog"
            aria-modal="true"
            aria-labelledby="delete-title"
          >
            <div className="modal-icon danger">
              <Trash2 size={24} />
            </div>
            <h2 id="delete-title">Delete {deleteTarget.name}?</h2>
            <p>
              This removes the saved connection and its credential entry from
              your vault. A backup of the previous vault is kept.
            </p>
            <div className="actions">
              <button
                className="secondary"
                disabled={busy}
                onClick={() => setDeleteTarget(null)}
              >
                Cancel
              </button>
              <button
                className="danger-button"
                disabled={busy}
                onClick={() => void remove()}
              >
                {busy ? "Deleting…" : "Delete connection"}
              </button>
            </div>
          </section>
        </div>
      )}
      {help && (
        <div className="modal-backdrop">
          <section
            className="modal"
            role="dialog"
            aria-modal="true"
            aria-labelledby="about-title"
          >
            <button
              className="modal-close"
              aria-label="Close"
              onClick={() => setHelp(false)}
            >
              <X size={19} />
            </button>
            <div className="modal-icon">
              <Anchor size={26} />
            </div>
            <div className="eyebrow">HARBOR · VERSION 0.5.0</div>
            <h2 id="about-title">A workspace you own.</h2>
            <p>
              Harbor stores credentials in a password-protected KeePass KDBX
              vault and connects directly to your SSH servers.
            </p>
            <div className="about-details">
              <p>
                <strong>Your vault</strong>
                <br />
                <span className="path-text">
                  {status.vaultPath ?? "Create or open a vault to get started."}
                </span>
              </p>
              <p>
                <strong>What is saved</strong>
                <br />
                Usernames and passwords live in KDBX. Connection names, hosts,
                ports, groups, and tags are indexed locally in SQLite and
                included in the vault.
              </p>
              <p>
                <strong>Available now</strong>
                <br />
                SSH passwords, private keys and OpenSSH user certificates,
                terminal tabs, local / remote port forwarding, and KDBX vaults
                with optional Windows Hello second-factor protection. Dynamic /
                SOCKS forwarding, cloud sync and FIDO / passkey SSH identities
                are planned.
              </p>
              <p>
                <strong>Vault compatibility</strong>
                <br />
                KDBX writing uses an experimental library. Use a dedicated
                Harbor vault and keep backups while this version is being
                tested.
              </p>
            </div>
            {status.unlocked && (
              <button
                className="secondary full-width"
                style={{ marginBottom: 10 }}
                onClick={() => {
                  setHelp(false);
                  setVaultSecurity(true);
                }}
              >
                <ShieldCheck size={15} />
                Vault security / Windows Hello
              </button>
            )}
            <button
              className="secondary full-width"
              onClick={() => setHelp(false)}
            >
              Back to workspace
            </button>
          </section>
        </div>
      )}
    </div>
  );
}

function VaultModal({
  mode,
  previousPath,
  onClose,
  onDone,
}: {
  mode: VaultMode;
  previousPath: string | null;
  onClose: () => void;
  onDone: (s: VaultStatus) => void;
}) {
  const [path, setPath] = useState(
    mode === "unlock" ? (previousPath ?? "") : "",
  );
  const [password, setPassword] = useState("");
  const [confirm, setConfirm] = useState("");
  const [keyFilePath, setKeyFilePath] = useState("");
  const [hello, setHello] = useState<HelloStatus | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const create = mode === "create";
  useEffect(() => {
    let active = true;
    const timer = setTimeout(() => {
      if (!create && isTauri() && path)
        void invoke<HelloStatus>("hello_status", { path })
          .then((result) => {
            if (active) setHello(result);
          })
          .catch(() => {
            if (active) setHello(null);
          });
    }, 300);
    return () => {
      active = false;
      clearTimeout(timer);
    };
  }, [path, create]);
  const pickKeyFile = async () => {
    try {
      const selected = await open({
        title: "Choose recovery / KeePass key file",
        multiple: false,
        filters: [
          { name: "KeePass key file", extensions: ["key", "keyx", "*"] },
        ],
      });
      if (typeof selected === "string") setKeyFilePath(selected);
    } catch (e) {
      setError(errorText(e));
    }
  };
  const pick = async () => {
    try {
      const selected = create
        ? await save({
            title: "Create Harbor vault",
            defaultPath: "Harbor.kdbx",
            filters: [{ name: "KeePass vault", extensions: ["kdbx"] }],
          })
        : await open({
            title: "Open KeePass vault",
            multiple: false,
            filters: [{ name: "KeePass vault", extensions: ["kdbx"] }],
          });
      if (typeof selected === "string") setPath(selected);
    } catch (e) {
      setError(errorText(e));
    }
  };
  const submit = async (e: FormEvent) => {
    e.preventDefault();
    setError("");
    if (create && password !== confirm) {
      setError("Master passwords do not match.");
      return;
    }
    setBusy(true);
    try {
      const useHello =
        !create && hello?.enabled && hello.supported && !keyFilePath;
      const result = await invoke<VaultStatus>(
        useHello ? "hello_unlock" : "unlock_vault",
        {
          path,
          password,
          create,
          keyFilePath: keyFilePath || null,
        },
      );
      setPassword("");
      setConfirm("");
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
        aria-labelledby="vault-title"
        onSubmit={(e) => void submit(e)}
      >
        <button
          type="button"
          className="modal-close"
          aria-label="Close"
          disabled={busy}
          onClick={onClose}
        >
          <X size={19} />
        </button>
        <div className="modal-icon">
          <FileKey2 size={25} />
        </div>
        <div className="eyebrow">YOUR ENCRYPTED WORKSPACE</div>
        <h2 id="vault-title">
          {create ? "Create your vault." : "Unlock your vault."}
        </h2>
        <p>
          {create
            ? "A portable KDBX file. One master password. Your credentials, under your control."
            : "Enter the master password for your KeePass-compatible vault."}
        </p>
        <label htmlFor="vault-path">Vault file</label>
        <div className="path-picker">
          <input
            id="vault-path"
            value={path}
            onChange={(e) => setPath(e.target.value)}
            placeholder={
              create ? "Choose where to save your vault" : "Choose a .kdbx file"
            }
            required
            disabled={busy}
          />
          <button
            type="button"
            className="secondary"
            onClick={() => void pick()}
            disabled={busy}
          >
            <Folder size={16} />
            Browse
          </button>
        </div>
        <label htmlFor="master-password">Master password</label>
        <input
          id="master-password"
          type="password"
          autoComplete={create ? "new-password" : "current-password"}
          minLength={create ? 12 : undefined}
          value={password}
          onChange={(e) => setPassword(e.target.value)}
          required
          disabled={busy}
          autoFocus
        />
        <span className="field-hint">
          {create
            ? "Use at least 12 characters. You will need this password on every device."
            : hello?.enabled && hello.supported
              ? "Your master password and Windows Hello are both required. Choose a recovery file below to use that instead of Hello."
              : "For a key-file / two-factor vault, also choose its recovery key file below."}
        </span>
        {create && (
          <>
            <label htmlFor="confirm-password">Confirm master password</label>
            <input
              id="confirm-password"
              type="password"
              autoComplete="new-password"
              value={confirm}
              onChange={(e) => setConfirm(e.target.value)}
              required
              disabled={busy}
            />
          </>
        )}
        {!create && (
          <>
            <label htmlFor="vault-keyfile">
              Recovery / KeePass key file (if required)
            </label>
            <div className="path-picker">
              <input
                id="vault-keyfile"
                value={keyFilePath}
                onChange={(e) => setKeyFilePath(e.target.value)}
                placeholder="Leave blank to use registered Windows Hello"
                disabled={busy}
              />
              <button
                type="button"
                className="secondary"
                disabled={busy}
                onClick={() => void pickKeyFile()}
              >
                <Folder size={15} />
                Browse
              </button>
            </div>
            {hello?.enabled && !hello.supported && (
              <p className="field-hint">
                Windows Hello is unavailable here. Use the master password and
                recovery key file.
              </p>
            )}
          </>
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
            onClick={onClose}
            disabled={busy}
          >
            Cancel
          </button>
          <button className="primary" disabled={busy}>
            {busy
              ? "Opening vault…"
              : create
                ? "Create vault"
                : hello?.enabled && hello.supported && !keyFilePath
                  ? "Password + Windows Hello"
                  : "Unlock vault"}
            <ArrowRight size={15} />
          </button>
        </div>
      </form>
    </div>
  );
}

function ConnectionModal({
  profile,
  folders,
  onClose,
  onDone,
}: {
  profile: Profile | null;
  folders: string[];
  onClose: () => void;
  onDone: (s: VaultStatus) => void;
}) {
  const [name, setName] = useState(profile?.name ?? "");
  const [host, setHost] = useState(profile?.host ?? "");
  const [port, setPort] = useState(String(profile?.port ?? 22));
  const [folder, setFolder] = useState(profile?.folder ?? "General");
  const [tagsText, setTagsText] = useState(profile?.tags.join(", ") ?? "");
  const [forwards, setForwards] = useState(profile?.forwards ?? []);
  const [startupSnippet, setStartupSnippet] = useState(
    profile?.startupSnippet ?? "",
  );
  const [snippets, setSnippets] = useState<Snippet[]>([]);
  const [snippetError, setSnippetError] = useState("");
  useEffect(() => {
    let alive = true;
    void invoke<Snippet[]>("list_snippets")
      .then((items) => {
        if (alive) setSnippets(items);
      })
      .catch((e) => {
        if (alive) setSnippetError(errorText(e));
      });
    return () => {
      alive = false;
    };
  }, []);
  const [authentication, setAuthentication] = useState(() =>
    newAuthDraft(profile),
  );
  const [username, setUsername] = useState("");
  const [password, setPassword] = useState("");
  const [replacePassword, setReplacePassword] = useState(!profile);
  const [loading, setLoading] = useState(!!profile);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  useEffect(() => {
    if (profile)
      void invoke<string>("connection_username", { id: profile.id })
        .then(setUsername)
        .catch((e) => setError(errorText(e)))
        .finally(() => setLoading(false));
  }, [profile]);
  const submit = async (e: FormEvent) => {
    e.preventDefault();
    setBusy(true);
    setError("");
    try {
      const result = await invoke<VaultStatus>("save_connection", {
        profile: {
          id: profile?.id ?? null,
          name,
          host,
          port: Number(port),
          folder,
          tags: tagsText
            .split(/[,;]/)
            .map((t) => t.trim())
            .filter(Boolean),
          username,
          password:
            authentication.method === "password" &&
            (replacePassword || profile?.authMethod !== "password")
              ? password
              : null,
          authentication: authenticationUpdate(authentication),
          forwards,
          startupSnippet: startupSnippet || null,
        },
      });
      setPassword("");
      setUsername("");
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
        aria-labelledby="connection-title"
        onSubmit={(e) => void submit(e)}
      >
        <button
          type="button"
          className="modal-close"
          aria-label="Close"
          onClick={onClose}
          disabled={busy}
        >
          <X size={19} />
        </button>
        <div className="modal-icon">
          <Server size={24} />
        </div>
        <div className="eyebrow">A PLACE IN YOUR WORKSPACE</div>
        <h2 id="connection-title">
          {profile ? "Edit connection." : "New connection."}
        </h2>
        <p>Save a server. Keep its credentials in your vault.</p>
        <fieldset disabled={busy || loading}>
          <label htmlFor="connection-name">Connection name</label>
          <input
            id="connection-name"
            placeholder="e.g. Production server"
            value={name}
            onChange={(e) => setName(e.target.value)}
            required
            maxLength={120}
            autoFocus
          />
          <div className="form-grid">
            <div>
              <label htmlFor="connection-host">Hostname or IP address</label>
              <input
                id="connection-host"
                placeholder="192.168.1.10"
                value={host}
                onChange={(e) => setHost(e.target.value)}
                required
              />
            </div>
            <div>
              <label htmlFor="connection-port">Port</label>
              <input
                id="connection-port"
                type="number"
                min="1"
                max="65535"
                value={port}
                onChange={(e) => setPort(e.target.value)}
                required
              />
            </div>
          </div>
          <label htmlFor="connection-folder">Group</label>
          <input
            id="connection-folder"
            list="folder-options"
            value={folder}
            onChange={(e) => setFolder(e.target.value)}
            maxLength={80}
          />
          <datalist id="folder-options">
            {folders.map((f) => (
              <option key={f} value={f} />
            ))}
          </datalist>
          <label htmlFor="connection-tags">Tags</label>
          <input
            id="connection-tags"
            value={tagsText}
            onChange={(e) => setTagsText(e.target.value)}
            placeholder="production, linux, work"
            aria-describedby="tags-help"
          />
          <p id="tags-help" className="field-hint">
            Separate tags with commas. Up to 20 tags, 40 characters each. Tags
            are saved in lowercase.
          </p>
          <div className="credentials-heading">
            <KeyRound size={14} />
            <span>CREDENTIALS</span>
            <span className="secure-label">
              <LockKeyhole size={10} />
              Saved in KDBX
            </span>
          </div>
          <label htmlFor="connection-user">Username</label>
          <input
            id="connection-user"
            value={username}
            onChange={(e) => setUsername(e.target.value)}
            autoComplete="off"
            required
          />
          <AuthenticationFields
            value={authentication}
            onChange={setAuthentication}
            profile={profile}
            onError={setError}
          />
          {authentication.method === "password" &&
            profile?.authMethod === "password" && (
              <label className="checkbox-label">
                <input
                  type="checkbox"
                  checked={replacePassword}
                  onChange={(e) => setReplacePassword(e.target.checked)}
                />
                Replace saved password
              </label>
            )}
          {authentication.method === "password" &&
            (replacePassword || profile?.authMethod !== "password") && (
              <>
                <label htmlFor="connection-password">Password</label>
                <input
                  id="connection-password"
                  type="password"
                  value={password}
                  onChange={(e) => setPassword(e.target.value)}
                  autoComplete="new-password"
                  required
                />
              </>
            )}
          <label htmlFor="host-startup-snippet">Startup snippet</label>
          <select
            id="host-startup-snippet"
            value={startupSnippet}
            onChange={(e) => setStartupSnippet(e.target.value)}
          >
            <option value="">None</option>
            {startupSnippet &&
              !snippets.some((s) => s.id === startupSnippet) && (
                <option value={startupSnippet}>
                  Unavailable snippet � choose another or None
                </option>
              )}
            {snippets.map((s) => (
              <option key={s.id} value={s.id}>
                {s.group} / {s.name}
              </option>
            ))}
          </select>
          <p className="field-hint">
            Runs automatically once after authentication and shell startup, on
            every new connection. Manage commands in Snippets.
          </p>
          {snippetError && (
            <p className="form-error" role="alert">
              {snippetError}
            </p>
          )}
          {snippets.find((s) => s.id === startupSnippet) && (
            <pre className="startup-preview">
              {snippets.find((s) => s.id === startupSnippet)?.body}
            </pre>
          )}
          <ForwardingEditor rules={forwards} onChange={setForwards} />
        </fieldset>
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
            Cancel
          </button>
          <button className="primary" disabled={busy || loading}>
            {busy ? "Encrypting & saving…" : "Save connection"}
            <Check size={15} />
          </button>
        </div>
      </form>
    </div>
  );
}
