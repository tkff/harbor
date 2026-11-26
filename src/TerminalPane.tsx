import { terminalOptions, type Preferences } from "./preferences";
import { copyTerminalText } from "./clipboard";
import { isTerminalCopyKey, isTerminalInterruptKey } from "./terminalKeys";
import Snippets from "./Snippets";
import ForwardingPanel from "./ForwardingPanel";
import { useEffect, useRef, useState } from "react";
import { Terminal } from "@xterm/xterm";
import { FitAddon } from "@xterm/addon-fit";
import "@xterm/xterm/css/xterm.css";
import { ShieldCheck, Unplug, Network, Code2, Copy } from "lucide-react";
import {
  Channel,
  invoke,
  errorText,
  type Session,
  type SshEvent,
  type LoginCredentials,
} from "./api";
import type { ForwardStatus } from "./api";

type HostKey = Extract<SshEvent, { type: "hostKey" }>;
export default function TerminalPane({
  session,
  preferences,
  active,
  onState,
  takeCredentials,
  onRetry,
  onEdit,
  onReconnect,
}: {
  session: Session;
  preferences: Preferences;
  active: boolean;
  onState: (id: string, state: Session["state"]) => void;
  takeCredentials: (id: string) => LoginCredentials | null;
  onRetry: (credentials: LoginCredentials) => void;
  onEdit: () => void;
  onReconnect: () => void;
}) {
  const mount = useRef<HTMLDivElement>(null);
  const terminal = useRef<Terminal | null>(null);
  const scheduleFit = useRef<(sync?: boolean, revealBottom?: boolean) => void>(
    () => {},
  );
  const [hasSelection, setHasSelection] = useState(false);
  const [copyStatus, setCopyStatus] = useState<{
    error: boolean;
    text: string;
  } | null>(null);
  const copyTimer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const copyAlive = useRef(true);
  useEffect(() => {
    copyAlive.current = true;
    return () => {
      copyAlive.current = false;
      if (copyTimer.current) clearTimeout(copyTimer.current);
    };
  }, []);
  const copySelection = async () => {
    const text = terminal.current?.getSelection();
    if (!text) return;
    try {
      await copyTerminalText(text);
      if (copyAlive.current)
        setCopyStatus({ error: false, text: "Copied to clipboard" });
    } catch (e) {
      if (copyAlive.current)
        setCopyStatus({ error: true, text: `Could not copy: ${errorText(e)}` });
    }
    if (copyAlive.current) {
      if (copyTimer.current) clearTimeout(copyTimer.current);
      copyTimer.current = setTimeout(() => setCopyStatus(null), 4000);
    }
  };
  const copyCallback = useRef(copySelection);
  copyCallback.current = copySelection;
  const [hostKey, setHostKey] = useState<HostKey | null>(null);
  const [authFailure, setAuthFailure] = useState<Extract<
    SshEvent,
    { type: "authenticationFailed" }
  > | null>(null);
  const [retryOpen, setRetryOpen] = useState(false);
  const [connectionFailure, setConnectionFailure] = useState<string | null>(
    null,
  );
  const [message, setMessage] = useState("Establishing a secure connection…");
  const [showForwards, setShowForwards] = useState(false);
  const [showSnippets, setShowSnippets] = useState(false);
  const [forwardStates, setForwardStates] = useState<
    Record<string, ForwardStatus>
  >({});
  const [pending, setPending] = useState<Record<string, boolean>>({});
  const rules = session.profile.forwards ?? [];
  const activeForwards = Object.values(forwardStates).filter(
    (s) => s.state === "active",
  ).length;
  const stateCallback = useRef(onState);
  stateCallback.current = onState;

  useEffect(() => {
    let alive = true;
    let connected = false;
    let reportedClose = false;
    const term = new Terminal({
      ...terminalOptions(preferences),
      allowProposedApi: false,
    });
    const fitter = new FitAddon();
    term.loadAddon(fitter);
    terminal.current = term;
    term.open(mount.current!);
    const selection = term.onSelectionChange(() => {
      if (alive) setHasSelection(term.hasSelection());
    });
    term.attachCustomKeyEventHandler((event) => {
      if (isTerminalInterruptKey(event, term.hasSelection())) {
        event.preventDefault();
        event.stopPropagation();
        if (event.type === "keydown") term.input("\x03", true);
        return false;
      }
      if (!isTerminalCopyKey(event, term.hasSelection())) return true;
      event.preventDefault();
      event.stopPropagation();
      if (event.type === "keydown" && !event.repeat)
        void copyCallback.current();
      return false;
    });
    let fitFrame = 0;
    let syncSize = false;
    let revealBottom = false;
    const queueFit = (sync = false, bottom = false) => {
      syncSize ||= sync;
      revealBottom ||= bottom;
      cancelAnimationFrame(fitFrame);
      fitFrame = requestAnimationFrame(() => {
        if (
          !alive ||
          !mount.current?.clientWidth ||
          !mount.current?.clientHeight
        )
          return;
        fitter.fit();
        term.refresh(0, term.rows - 1);
        if (revealBottom) term.scrollToBottom();
        if (syncSize && connected)
          void invoke("ssh_resize", {
            sessionId: session.id,
            cols: term.cols,
            rows: term.rows,
          }).catch(() => {});
        syncSize = false;
        revealBottom = false;
      });
    };
    scheduleFit.current = queueFit;
    fitter.fit();
    void document.fonts.ready.then(() => {
      if (alive) queueFit(true);
    });
    const showClosed = (text: string) => {
      if (!alive || reportedClose) return;
      reportedClose = true;
      if (!connected) setConnectionFailure(text);
      connected = false;
      setHostKey(null);
      setMessage(text);
      stateCallback.current(session.id, "closed");
      term.writeln(
        `\r\n\x1b[90m[${text.replace(/[\x00-\x1f\x7f-\x9f]/g, " ")}]\x1b[0m`,
      );
    };
    const events = new Channel<SshEvent>();
    events.onmessage = (event) => {
      if (!alive || reportedClose) return;
      if (event.type === "data") term.write(new Uint8Array(event.bytes));
      else if (event.type === "hostKey") {
        setHostKey(event);
        setMessage("Waiting for server identity verification");
      } else if (event.type === "connected") {
        connected = true;
        setMessage("Encrypted SSH session");
        stateCallback.current(session.id, "connected");
        queueFit(true, true);
        term.focus();
      } else if (event.type === "authenticationFailed") {
        setAuthFailure(event);
        setRetryOpen(true);
      } else if (event.type === "forward") {
        setForwardStates((prev) => ({
          ...prev,
          [event.ruleId]: { state: event.state, message: event.message },
        }));
      } else {
        setForwardStates({});
        showClosed(event.message);
      }
    };
    // Serialize keyboard chunks to preserve ordering and bound paste payloads.
    let queue = Promise.resolve();
    const data = term.onData((text) => {
      if (!connected) return;
      const bytes = new TextEncoder().encode(text);
      for (let i = 0; i < bytes.length; i += 16384) {
        const chunk = Array.from(bytes.subarray(i, i + 16384));
        queue = queue
          .then(async () => {
            if (alive && connected)
              await invoke("ssh_write", { sessionId: session.id, data: chunk });
          })
          .catch((e) => showClosed(errorText(e)));
      }
    });
    const resize = term.onResize(({ cols, rows }) => {
      if (connected)
        void invoke("ssh_resize", { sessionId: session.id, cols, rows }).catch(
          () => {},
        );
    });
    const observer = new ResizeObserver(() => queueFit());
    observer.observe(mount.current!);
    void invoke("ssh_connect", {
      sessionId: session.id,
      profileId: session.profile.id,
      cols: term.cols,
      rows: term.rows,
      events,
      credentials: takeCredentials(session.id),
    }).catch((e) => showClosed(errorText(e)));
    return () => {
      alive = false;
      connected = false;
      void invoke("ssh_close", { sessionId: session.id }).catch(() => {});
      observer.disconnect();
      cancelAnimationFrame(fitFrame);
      scheduleFit.current = () => {};
      selection.dispose();
      data.dispose();
      resize.dispose();
      term.dispose();
    };
  }, [session.id]);

  useEffect(() => {
    if (active) {
      scheduleFit.current(true);
      terminal.current?.focus();
    }
  }, [active]);

  useEffect(() => {
    if (!terminal.current) return;
    terminal.current.options = terminalOptions(preferences);
    if (active) scheduleFit.current(true);
  }, [preferences, active]);

  const approve = async (accepted: boolean) => {
    setHostKey(null);
    try {
      await invoke("ssh_approve", { sessionId: session.id, accepted });
    } catch (e) {
      setMessage(errorText(e));
    }
  };
  const toggleForward = async (ruleId: string, start: boolean) => {
    setPending((p) => ({ ...p, [ruleId]: true }));
    try {
      await invoke("ssh_forward", { sessionId: session.id, ruleId, start });
    } catch (e) {
      setForwardStates((prev) => ({
        ...prev,
        [ruleId]: {
          state: prev[ruleId]?.state ?? "error",
          message: errorText(e),
        },
      }));
    } finally {
      setPending((p) => ({ ...p, [ruleId]: false }));
    }
  };
  return (
    <section
      className={`terminal-pane ${active ? "active" : ""}`}
      aria-label={`${session.profile.name} terminal`}
    >
      <div className="terminal-toolbar">
        <span>
          <span className={`status-dot ${session.state}`} />
          {session.profile.host}
          <span className="dim">:{session.profile.port}</span>
        </span>
        <span className="terminal-encryption">
          {session.state === "connected" ? (
            <ShieldCheck size={13} />
          ) : (
            <Unplug size={13} />
          )}
          {message}
        </span>
        <button
          className="forward-toggle"
          disabled={!hasSelection}
          onMouseDown={(event) => event.preventDefault()}
          onClick={() => void copySelection()}
          title="Copy selected text (Ctrl+C, Ctrl+Shift+C, or Cmd+C). Hold Shift while selecting in mouse-aware terminal apps."
          aria-label="Copy selected terminal text"
        >
          <Copy size={13} /> Copy
        </button>
        <button
          className="forward-toggle"
          onClick={() => setShowForwards((v) => !v)}
          aria-expanded={showForwards}
        >
          <Network size={13} />
          Port forwarding
          <span>
            {activeForwards}/{rules.length}
          </span>
        </button>
        <button
          className="forward-toggle"
          disabled={session.state !== "connected"}
          onClick={() => setShowSnippets(true)}
        >
          <Code2 size={13} /> Snippets
        </button>
        {session.state === "closed" && (
          <>
            {authFailure && (
              <button
                className="forward-toggle"
                onClick={() => setRetryOpen(true)}
              >
                Correct credentials
              </button>
            )}
            <button className="forward-toggle" onClick={onEdit}>
              Edit saved host
            </button>
          </>
        )}
      </div>
      {copyStatus && (
        <div
          className={`terminal-copy-status ${copyStatus.error ? "is-error" : ""}`}
          role={copyStatus.error ? "alert" : "status"}
        >
          {copyStatus.text}
        </div>
      )}
      {connectionFailure && !(authFailure && retryOpen) && (
        <div className="connection-failure" role="alert">
          <div>
            <h3>Connection failed</h3>
            <p className="connection-failure-target">
              {session.profile.name} - {session.profile.host}:
              {session.profile.port}
            </p>
            <p className="connection-failure-reason">{connectionFailure}</p>
          </div>
          <div className="connection-failure-actions">
            <button className="primary" onClick={onEdit}>
              Edit host
            </button>
            <button className="secondary" onClick={onReconnect}>
              Retry connection
            </button>
          </div>
        </div>
      )}
      {showSnippets && session.state === "connected" && active && (
        <Snippets
          sessionName={session.profile.name}
          onClose={() => setShowSnippets(false)}
          onRun={(body) => {
            if (session.state !== "connected" || !terminal.current) return;
            terminal.current.paste(
              body.replace(/\r\n/g, "\n").replace(/[\r\n]+$/, ""),
            );
            terminal.current.input("\r", true);
            terminal.current.focus();
          }}
        />
      )}
      {session.profile.sessionLogging && session.state === "connected" && (
        <div className="recording-indicator" role="status">
          <span /> Recording encrypted session log
        </div>
      )}
      <div
        className="terminal-content"
        style={{ background: terminalOptions(preferences).theme?.background }}
      >
        <div className="terminal-mount" ref={mount} />
        {showForwards && (
          <ForwardingPanel
            rules={rules}
            states={forwardStates}
            pending={pending}
            connected={session.state === "connected"}
            onToggle={(id, start) => void toggleForward(id, start)}
            onClose={() => setShowForwards(false)}
          />
        )}
      </div>
      {authFailure && retryOpen && active && (
        <CredentialRetry
          failure={authFailure}
          host={session.profile.host}
          onCancel={() => setRetryOpen(false)}
          onEdit={() => {
            setRetryOpen(false);
            onEdit();
          }}
          onRetry={(credentials) => {
            setRetryOpen(false);
            onRetry(credentials);
          }}
        />
      )}
      {hostKey && (
        <div className="terminal-overlay">
          <div className="identity-card">
            <div className="modal-icon">
              <ShieldCheck size={25} />
            </div>
            <h2>Verify this server</h2>
            <p>
              This is your first connection to{" "}
              <strong>
                {hostKey.host}:{hostKey.port}
              </strong>
              . Compare this fingerprint with the server administrator before
              trusting it.
            </p>
            <label>SERVER KEY · SHA-256</label>
            <code className="fingerprint">{hostKey.fingerprint}</code>
            <p className="small muted">
              Harbor will remember this identity and block future connections if
              it changes.
            </p>
            <div className="actions">
              <button className="secondary" onClick={() => void approve(false)}>
                Cancel connection
              </button>
              <button className="primary" onClick={() => void approve(true)}>
                Trust & connect
              </button>
            </div>
          </div>
        </div>
      )}
    </section>
  );
}

function CredentialRetry({
  failure,
  host,
  onCancel,
  onEdit,
  onRetry,
}: {
  failure: Extract<SshEvent, { type: "authenticationFailed" }>;
  host: string;
  onCancel: () => void;
  onEdit: () => void;
  onRetry: (credentials: LoginCredentials) => void;
}) {
  const [username, setUsername] = useState(failure.username);
  const [password, setPassword] = useState("");
  return (
    <div className="terminal-overlay">
      <form
        className="identity-card credential-retry"
        role="dialog"
        aria-modal="true"
        aria-labelledby="retry-title"
        onSubmit={(event) => {
          event.preventDefault();
          const credentials = {
            username,
            password: failure.method === "password" ? password : null,
          };
          setPassword("");
          onRetry(credentials);
        }}
      >
        <h2 id="retry-title">Login was rejected.</h2>
        <p>Check the credentials for {host} and try again.</p>
        <label htmlFor="retry-username">SSH username</label>
        <input
          id="retry-username"
          autoComplete="off"
          required
          maxLength={256}
          value={username}
          onChange={(e) => setUsername(e.target.value)}
        />
        {failure.method === "password" ? (
          <>
            <label htmlFor="retry-password">SSH password</label>
            <input
              id="retry-password"
              type="password"
              autoComplete="off"
              required
              value={password}
              onChange={(e) => setPassword(e.target.value)}
            />
          </>
        ) : (
          <p>
            The saved{" "}
            {failure.method === "certificate"
              ? "OpenSSH certificate and private key"
              : "private key"}{" "}
            will be used with this username. Edit the saved host to replace the
            identity.
          </p>
        )}
        <p className="small muted">
          Retry credentials apply only to this connection. Edit the saved host
          to update the vault.
        </p>
        <div className="actions">
          <button type="button" className="secondary" onClick={onCancel}>
            Cancel
          </button>
          <button type="button" className="secondary" onClick={onEdit}>
            Edit saved host
          </button>
          <button className="primary">Retry connection</button>
        </div>
      </form>
    </div>
  );
}
