import { useEffect, useState } from "react";
import { X, RefreshCw } from "lucide-react";
import { invoke, errorText } from "./api";
export type LogSummary = {
  id: string;
  name: string;
  host: string;
  port: number;
  startedAt: number;
  logType?: string;
};
type LogItem = { id: string; session: LogSummary | null; error: string | null };
type LogRecord =
  | { kind: "status"; at: number; event: string }
  | { kind: "output"; at: number; bytes: number[] }
  | { kind: "end"; at: number; reason: string };
type SavedLog = {
  session: LogSummary;
  records: LogRecord[];
  complete: boolean;
};
function text(log: SavedLog) {
  const decoder = new TextDecoder();
  let output = "";
  for (const record of log.records)
    if (record.kind === "output")
      output += decoder.decode(Uint8Array.from(record.bytes), { stream: true });
  output += decoder.decode();
  return output
    .replace(/\x1b\][\s\S]*?(?:\x07|\x1b\\)/g, "")
    .replace(/\x1b\[[0-?]*[ -/]*[@-~]/g, "")
    .replace(/[\x00-\x08\x0b\x0c\x0e-\x1f\x7f-\x9f]/g, "");
}
export default function SessionLogs({
  onClose,
  kind = "session",
}: {
  onClose: () => void;
  kind?: "session" | "connection";
}) {
  const title = kind === "connection" ? "Connection history" : "Session logs";
  const [items, setItems] = useState<LogItem[]>([]);
  const [selected, setSelected] = useState<SavedLog | null>(null);
  const [search, setSearch] = useState("");
  const [error, setError] = useState("");
  const [busy, setBusy] = useState(true);
  const [deleting, setDeleting] = useState<string | null>(null);
  useEffect(() => {
    let alive = true;
    invoke<LogItem[]>("list_session_logs")
      .then((items) => {
        if (alive) setItems(items);
      })
      .catch((e) => {
        if (alive) setError(errorText(e));
      })
      .finally(() => {
        if (alive) setBusy(false);
      });
    return () => {
      alive = false;
    };
  }, []);
  async function refresh() {
    setBusy(true);
    setError("");
    try {
      const refreshed = await invoke<LogItem[]>("list_session_logs");
      setItems(refreshed);
      if (selected) {
        const id = selected.session.id;
        setSelected(null);
        if (refreshed.some((item) => item.id === id))
          setSelected(await invoke<SavedLog>("read_session_log", { id }));
      }
    } catch (e) {
      setError(errorText(e));
    } finally {
      setBusy(false);
    }
  }
  async function view(id: string) {
    setBusy(true);
    setError("");
    setSelected(null);
    setDeleting(null);
    try {
      setSelected(await invoke<SavedLog>("read_session_log", { id }));
    } catch (e) {
      setError(errorText(e));
    } finally {
      setBusy(false);
    }
  }
  async function remove(id: string) {
    setBusy(true);
    setError("");
    try {
      await invoke("delete_session_log", { id });
      setItems((items) => items.filter((item) => item.id !== id));
      setSelected(null);
      setDeleting(null);
    } catch (e) {
      setError(errorText(e));
    } finally {
      setBusy(false);
    }
  }
  const filteredItems = items.filter(
    (item) => !item.session || (item.session.logType ?? "session") === kind,
  );
  const output = selected ? text(selected) : "";
  const end = selected?.records.at(-1);
  return (
    <div className="modal-backdrop">
      <section
        className="modal session-logs-modal"
        role="dialog"
        aria-modal="true"
        aria-labelledby="logs-title"
      >
        <button
          className="modal-close"
          aria-label={`Close ${title.toLowerCase()}`}
          onClick={onClose}
        >
          <X size={19} />
        </button>
        <h2 id="logs-title">{title}.</h2>
        <p>
          Encrypted on this device. Unlock the matching vault to read them.
          {kind === "session"
            ? "Output may contain sensitive information echoed by the server."
            : "Connection history contains outcomes and timestamps, without terminal output."}
        </p>
        <div className="snippet-controls">
          <input
            aria-label={`Search ${title.toLowerCase()}`}
            placeholder="Search host or connection"
            value={search}
            onChange={(e) => setSearch(e.target.value)}
          />
          <button
            className="secondary"
            disabled={busy}
            onClick={() => void refresh()}
          >
            <RefreshCw size={14} /> Refresh
          </button>
        </div>
        {error && (
          <div className="form-error" role="alert">
            {error}
          </div>
        )}
        {busy && <p role="status">Loading logs...</p>}
        <div className="log-list">
          {filteredItems
            .filter((item) =>
              `${item.session?.name} ${item.session?.host} ${item.id}`
                .toLowerCase()
                .includes(search.toLowerCase()),
            )
            .map((item) => (
              <div className="log-row" key={item.id}>
                <button
                  className="snippet-pick"
                  disabled={busy}
                  onClick={() => void view(item.id)}
                >
                  <strong>{item.session?.name ?? "Unreadable log"}</strong>
                  <span>
                    {item.session
                      ? `${item.session.host}:${item.session.port} - ${new Date(item.session.startedAt).toLocaleString()}`
                      : item.error}
                  </span>
                </button>
                <button
                  className="secondary"
                  aria-label={`Delete log ${item.session?.name ?? item.id}`}
                  disabled={busy}
                  onClick={() => setDeleting(item.id)}
                >
                  Delete
                </button>
              </div>
            ))}
          {!busy && !filteredItems.length && (
            <p>
              No logs yet. Check logging defaults or a host's connection
              settings, then connect.
            </p>
          )}
        </div>
        {deleting && (
          <div className="snippet-delete">
            <p>
              Delete this encrypted log from this device? This cannot be undone.
              Close all SSH sessions first.
            </p>
            <button
              className="secondary"
              disabled={busy}
              onClick={() => setDeleting(null)}
            >
              Cancel
            </button>
            <button
              className="primary"
              disabled={busy}
              onClick={() => void remove(deleting)}
            >
              Delete log permanently
            </button>
          </div>
        )}
        {selected && (
          <>
            <h3>
              {selected.session.name} - {selected.session.host}:
              {selected.session.port}
            </h3>
            <p className="field-hint">
              {selected.complete
                ? `Finished ${end?.kind === "end" ? new Date(end.at).toLocaleString() : ""}. ${end?.kind === "end" ? end.reason : ""}`
                : "Incomplete or still recording. The authenticated records shown may be only part of this session. Refresh after the session closes."}
            </p>
            <pre
              className="session-log-output"
              tabIndex={0}
              aria-label={
                kind === "connection"
                  ? "Connection events"
                  : "Session log output"
              }
            >
              {kind === "connection"
                ? selected.records
                    .map(
                      (record) =>
                        `${new Date(record.at).toLocaleString()}  ${record.kind === "status" ? record.event : record.kind === "end" ? record.reason : ""}`,
                    )
                    .join("\n")
                : output.slice(0, 2_000_000) || "No terminal output recorded."}
            </pre>
            {output.length > 2_000_000 && (
              <p className="field-hint">
                Preview limited to two million characters. The encrypted file
                retains the full recorded output.
              </p>
            )}
            {kind === "session" && (
              <p className="field-hint">
                Text preview removes terminal escape sequences. Full-screen
                applications may appear as repeated screen updates.
              </p>
            )}
          </>
        )}
      </section>
    </div>
  );
}
