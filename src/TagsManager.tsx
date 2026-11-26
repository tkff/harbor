import { useEffect, useState } from "react";
import { X } from "lucide-react";
import { invoke, errorText } from "./api";

export type TagChange = { oldName: string | null; name: string | null };
export default function TagsManager({
  onClose,
  onChanged,
}: {
  onClose: () => void;
  onChanged: (tags: string[], change: TagChange) => void;
}) {
  const [tags, setTags] = useState<string[]>([]);
  const [name, setName] = useState("");
  const [editing, setEditing] = useState<string | null>(null);
  const [deleting, setDeleting] = useState<string | null>(null);
  const [search, setSearch] = useState("");
  const [busy, setBusy] = useState(true);
  const [error, setError] = useState("");
  useEffect(() => {
    let alive = true;
    invoke<string[]>("list_tags")
      .then((tags) => {
        if (alive) setTags(tags);
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
  const save = async () => {
    setBusy(true);
    setError("");
    try {
      const result = await invoke<string[]>("save_tag", {
        oldName: editing,
        name,
      });
      setTags(result);
      onChanged(result, { oldName: editing, name: name.trim().toLowerCase() });
      setEditing(null);
      setName("");
    } catch (e) {
      setError(errorText(e));
    } finally {
      setBusy(false);
    }
  };
  const remove = async () => {
    if (!deleting) return;
    setBusy(true);
    setError("");
    try {
      const result = await invoke<string[]>("delete_tag", { name: deleting });
      setTags(result);
      onChanged(result, { oldName: deleting, name: null });
      if (editing === deleting) {
        setEditing(null);
        setName("");
      }
      setDeleting(null);
    } catch (e) {
      setError(errorText(e));
    } finally {
      setBusy(false);
    }
  };
  return (
    <div className="modal-backdrop tag-manager-backdrop">
      <section
        className="modal small-modal"
        role="dialog"
        aria-modal="true"
        aria-labelledby="tags-title"
      >
        <button
          type="button"
          className="modal-close"
          aria-label="Close tag manager"
          disabled={busy}
          onClick={onClose}
        >
          <X size={19} />
        </button>
        <h2 id="tags-title">Manage tags.</h2>
        <p>
          Tags belong to this vault. Renaming updates all assigned hosts. Unused
          tags stay available.
        </p>
        <label htmlFor="tag-name">
          {editing ? `Rename ${editing}` : "New tag"}
        </label>
        <div className="snippet-controls">
          <input
            id="tag-name"
            value={name}
            onChange={(e) => setName(e.target.value)}
            maxLength={40}
            onKeyDown={(e) => {
              if (e.key === "Enter") {
                e.preventDefault();
                if (!busy && name.trim()) void save();
              }
            }}
          />
          <button
            type="button"
            className="primary"
            disabled={busy || !name.trim()}
            onClick={() => void save()}
          >
            {editing ? "Save name" : "Create tag"}
          </button>
          {editing && (
            <button
              type="button"
              className="secondary"
              disabled={busy}
              onClick={() => {
                setEditing(null);
                setName("");
              }}
            >
              Cancel rename
            </button>
          )}
        </div>
        <p className="field-hint">
          Lowercase, up to 40 characters. No commas or semicolons.
        </p>
        <input
          aria-label="Search tags"
          placeholder="Search tags"
          value={search}
          onChange={(e) => setSearch(e.target.value)}
        />
        {error && (
          <div className="form-error" role="alert">
            {error}
          </div>
        )}
        {busy && <p role="status">Loading tags...</p>}
        <div className="log-list">
          {tags
            .filter((tag) => tag.toLowerCase().includes(search.toLowerCase()))
            .map((tag) => (
              <div className="log-row" key={tag}>
                <strong>{tag}</strong>
                <div className="tag-actions">
                  <button
                    type="button"
                    className="secondary"
                    disabled={busy}
                    aria-label={`Rename tag ${tag}`}
                    onClick={() => {
                      setEditing(tag);
                      setName(tag);
                      setDeleting(null);
                    }}
                  >
                    Rename
                  </button>
                  <button
                    type="button"
                    className="secondary"
                    disabled={busy}
                    aria-label={`Delete tag ${tag}`}
                    onClick={() => setDeleting(tag)}
                  >
                    Delete
                  </button>
                </div>
              </div>
            ))}
          {!busy && !tags.length && (
            <p>No tags yet. Create your first tag above.</p>
          )}
        </div>
        {deleting && (
          <div className="snippet-delete" role="alert">
            <p>
              Delete tag <strong>{deleting}</strong> and remove it from every
              host? Hosts and their credentials are kept.
            </p>
            <button
              type="button"
              className="secondary"
              disabled={busy}
              onClick={() => setDeleting(null)}
            >
              Cancel
            </button>
            <button
              type="button"
              className="primary"
              disabled={busy}
              onClick={() => void remove()}
            >
              Delete tag
            </button>
          </div>
        )}
      </section>
    </div>
  );
}
