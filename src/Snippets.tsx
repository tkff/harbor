import { useEffect, useState, type FormEvent } from "react";
import { Code2, Plus, X } from "lucide-react";
import { invoke, errorText, type Snippet } from "./api";

type Props = {
  onClose: () => void;
  onRun?: (body: string) => void;
  sessionName?: string;
};
export default function Snippets({ onClose, onRun, sessionName }: Props) {
  const [snippets, setSnippets] = useState<Snippet[]>([]);
  const [search, setSearch] = useState("");
  const [group, setGroup] = useState("");
  const [selected, setSelected] = useState<string | null>(null);
  const [editor, setEditor] = useState<Snippet | "new" | null>(null);
  const [draft, setDraft] = useState("");
  const [deleting, setDeleting] = useState<string | null>(null);
  const [error, setError] = useState("");
  const [busy, setBusy] = useState(true);
  useEffect(() => {
    let alive = true;
    invoke<Snippet[]>("list_snippets")
      .then((s) => {
        if (alive) setSnippets(s);
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
  const groups = [...new Set(snippets.map((s) => s.group))].sort();
  const shown = snippets.filter(
    (s) =>
      (!group || s.group === group) &&
      `${s.name} ${s.group} ${s.tags.join(" ")}`
        .toLowerCase()
        .includes(search.toLowerCase()),
  );
  const picked = snippets.find((s) => s.id === selected);
  async function remove(id: string) {
    setBusy(true);
    setError("");
    try {
      setSnippets(await invoke<Snippet[]>("delete_snippet", { id }));
      setDeleting(null);
      if (selected === id) {
        setSelected(null);
        setDraft("");
      }
    } catch (e) {
      setError(errorText(e));
    } finally {
      setBusy(false);
    }
  }
  function run() {
    if (
      !draft.trim() ||
      new TextEncoder().encode(draft).length > 65536 ||
      /[\x00-\x08\x0b\x0c\x0e-\x1f\x7f-\x9f]/.test(draft)
    ) {
      setError(
        "Enter commands up to 64 KiB without terminal control characters.",
      );
      return;
    }
    onRun?.(draft);
    onClose();
  }
  return (
    <div className="modal-backdrop">
      <section
        className="modal snippets-modal"
        role="dialog"
        aria-modal="true"
        aria-labelledby="snippets-title"
      >
        <button
          className="modal-close"
          aria-label="Close snippets"
          onClick={onClose}
          disabled={busy}
        >
          <X size={19} />
        </button>
        <div className="modal-icon">
          <Code2 size={24} />
        </div>
        <h2 id="snippets-title">Command snippets.</h2>
        <p>Reusable commands, encrypted in your KDBX vault.</p>
        {error && (
          <div role="alert" className="form-error">
            {error}
          </div>
        )}
        {editor ? (
          <SnippetEditor
            key={editor === "new" ? "new" : editor.id}
            snippet={editor === "new" ? null : editor}
            groups={groups}
            onCancel={() => setEditor(null)}
            onSaved={(items) => {
              setSnippets(items);
              setEditor(null);
              setSelected(null);
              setDraft("");
            }}
          />
        ) : (
          <>
            <div className="snippet-controls">
              <input
                aria-label="Search snippets"
                placeholder="Search name, group, or tags"
                value={search}
                onChange={(e) => setSearch(e.target.value)}
              />
              <select
                aria-label="Snippet group filter"
                value={group}
                onChange={(e) => setGroup(e.target.value)}
              >
                <option value="">All groups</option>
                {groups.map((g) => (
                  <option key={g}>{g}</option>
                ))}
              </select>
              <button
                className="primary"
                onClick={() => setEditor("new")}
                disabled={busy}
              >
                <Plus size={14} /> New snippet
              </button>
            </div>
            {busy && <p role="status">Loading snippets...</p>}
            <div className="snippet-list">
              {shown.map((s) => (
                <div
                  className={`snippet-row ${s.id === selected ? "selected" : ""}`}
                  key={s.id}
                >
                  <button
                    className="snippet-pick"
                    onClick={() => {
                      setSelected(s.id);
                      setDraft(s.body);
                      setDeleting(null);
                    }}
                  >
                    <strong>{s.name}</strong>
                    <span>
                      {s.group}
                      {s.tags.length ? ` · ${s.tags.join(", ")}` : ""}
                    </span>
                  </button>
                  <button
                    className="secondary"
                    onClick={() => setEditor(s)}
                    disabled={busy}
                    aria-label={`Edit snippet ${s.name}`}
                  >
                    Edit
                  </button>
                  <button
                    className="secondary"
                    onClick={() => setDeleting(s.id)}
                    disabled={busy}
                    aria-label={`Delete snippet ${s.name}`}
                  >
                    Delete
                  </button>
                </div>
              ))}
              {!busy && !shown.length && (
                <p>
                  {snippets.length
                    ? "No snippets match these filters."
                    : "Save your first reusable command with New snippet."}
                </p>
              )}
            </div>
            {deleting && (
              <div className="snippet-delete">
                <p>
                  Delete this snippet? The previous vault backup retains the
                  prior version.
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
                  Delete snippet
                </button>
              </div>
            )}
            {picked && (
              <div className="snippet-preview">
                <label htmlFor="snippet-preview">
                  {onRun ? `Review commands for ${sessionName}` : picked.name}
                </label>
                <textarea
                  id="snippet-preview"
                  spellCheck={false}
                  value={draft}
                  readOnly={!onRun}
                  onChange={(e) => setDraft(e.target.value)}
                  rows={7}
                />
                {onRun ? (
                  <>
                    <p className="field-hint">
                      Runs in the current terminal context. Multi-line commands
                      execute in order. Review the commands and the current
                      shell before running. Edits here apply only to this run.
                    </p>
                    <button
                      className="primary"
                      onClick={run}
                      disabled={busy || !draft.trim()}
                    >
                      Run in this session
                    </button>
                  </>
                ) : (
                  <p className="field-hint">
                    Open Snippets from an SSH terminal to run this command.
                  </p>
                )}
              </div>
            )}
          </>
        )}
      </section>
    </div>
  );
}
function SnippetEditor({
  snippet,
  groups,
  onSaved,
  onCancel,
}: {
  snippet: Snippet | null;
  groups: string[];
  onSaved: (items: Snippet[]) => void;
  onCancel: () => void;
}) {
  const [name, setName] = useState(snippet?.name ?? "");
  const [group, setGroup] = useState(snippet?.group ?? "General");
  const [tags, setTags] = useState(snippet?.tags.join(", ") ?? "");
  const [body, setBody] = useState(snippet?.body ?? "");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  async function save(e: FormEvent) {
    e.preventDefault();
    setBusy(true);
    setError("");
    try {
      onSaved(
        await invoke<Snippet[]>("save_snippet", {
          snippet: {
            id: snippet?.id ?? null,
            name,
            group,
            tags: tags
              .split(/[,;]/)
              .map((t) => t.trim())
              .filter(Boolean),
            body,
          },
        }),
      );
    } catch (e) {
      setError(errorText(e));
    } finally {
      setBusy(false);
    }
  }
  return (
    <form onSubmit={(e) => void save(e)}>
      {error && (
        <div className="form-error" role="alert">
          {error}
        </div>
      )}
      <fieldset disabled={busy}>
        <label htmlFor="snippet-name">Snippet name</label>
        <input
          id="snippet-name"
          required
          maxLength={120}
          value={name}
          onChange={(e) => setName(e.target.value)}
        />
        <label htmlFor="snippet-group">Group</label>
        <input
          id="snippet-group"
          list="snippet-groups"
          maxLength={80}
          value={group}
          onChange={(e) => setGroup(e.target.value)}
        />
        <datalist id="snippet-groups">
          {groups.map((g) => (
            <option key={g} value={g} />
          ))}
        </datalist>
        <label htmlFor="snippet-tags">Tags</label>
        <input
          id="snippet-tags"
          value={tags}
          onChange={(e) => setTags(e.target.value)}
          placeholder="maintenance, linux"
        />
        <p className="field-hint">
          Comma-separated tags. Up to 20 tags, 40 characters each.
        </p>
        <label htmlFor="snippet-body">Commands</label>
        <textarea
          id="snippet-body"
          spellCheck={false}
          required
          rows={9}
          value={body}
          onChange={(e) => setBody(e.target.value)}
          placeholder="df -h"
        />
        <p className="field-hint">
          Use commands for your remote shell. Saving does not run them. Changes
          also apply to hosts using this snippet at their next startup.
        </p>
      </fieldset>
      <div className="actions">
        <button
          type="button"
          className="secondary"
          disabled={busy}
          onClick={onCancel}
        >
          Cancel
        </button>
        <button className="primary" disabled={busy}>
          {busy ? "Saving..." : "Save snippet"}
        </button>
      </div>
    </form>
  );
}
