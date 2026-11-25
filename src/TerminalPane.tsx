import Snippets from "./Snippets";
import ForwardingPanel from "./ForwardingPanel";
import { useEffect, useRef, useState } from "react";
import { Terminal } from "@xterm/xterm";
import { FitAddon } from "@xterm/addon-fit";
import "@xterm/xterm/css/xterm.css";
import { ShieldCheck, Unplug, Network, Code2 } from "lucide-react";
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
  active,
  onState,
  takeCredentials,
  onRetry,
  onEdit,
}: {
  session: Session;
  active: boolean;
  onState: (id: string, state: Session["state"]) => void;
  takeCredentials: (id: string) => LoginCredentials | null;
  onRetry: (credentials: LoginCredentials) => void;
  onEdit: () => void;
}) {
  const mount = useRef<HTMLDivElement>(null);
  const terminal = useRef<Terminal | null>(null);
  const fit = useRef<FitAddon | null>(null);
  const [hostKey, setHostKey] = useState<HostKey | null>(null);
  const [authFailure, setAuthFailure] = useState<Extract<
    SshEvent,
    { type: "authenticationFailed" }
  > | null>(null);
  const [retryOpen, setRetryOpen] = useState(false);
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
      cursorBlink: true,
      cursorStyle: "bar",
      fontSize: 14,
      lineHeight: 1.35,
      fontFamily:
        '"Cascadia Code", "SFMono-Regular", Consolas, "Liberation Mono", monospace',
      scrollback: 5000,
      allowProposedApi: false,
      theme: {
        background: "#10191f",
        foreground: "#d9e3e5",
        cursor: "#6cd5bb",
        selectionBackground: "#33554f",
        black: "#18262d",
        red: "#ed8f8a",
        green: "#8dd6af",
        yellow: "#e5c68c",
        blue: "#8ebbdc",
        magenta: "#c6a3d6",
        cyan: "#7fc9c8",
        white: "#d9e3e5",
      },
    });
    const fitter = new FitAddon();
    term.loadAddon(fitter);
    terminal.current = term;
    fit.current = fitter;
    term.open(mount.current!);
    fitter.fit();
    const showClosed = (text: string) => {
      if (!alive || reportedClose) return;
      reportedClose = true;
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
        fitter.fit();
        term.focus();
        void invoke("ssh_resize", {
          sessionId: session.id,
          cols: term.cols,
          rows: term.rows,
        }).catch(() => {});
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
    const observer = new ResizeObserver(() => {
      if (mount.current?.clientWidth) fitter.fit();
    });
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
      data.dispose();
      resize.dispose();
      term.dispose();
    };
  }, [session.id]);

  useEffect(() => {
    if (active)
      requestAnimationFrame(() => {
        fit.current?.fit();
        terminal.current?.focus();
      });
  }, [active]);

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
      <div className="terminal-content">
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
