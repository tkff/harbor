import { useEffect, useState } from "react";
import { X } from "lucide-react";
import { invoke, errorText } from "./api";
type KnownHost = { host: string; port: number; fingerprint: string };
export default function KnownHosts({ onClose }: { onClose: () => void }) {
  const [hosts, setHosts] = useState<KnownHost[]>([]);
  const [search, setSearch] = useState("");
  const [error, setError] = useState("");
  const [loading, setLoading] = useState(true);
  useEffect(() => {
    let alive = true;
    invoke<KnownHost[]>("known_hosts")
      .then((items) => {
        if (alive) setHosts(items);
      })
      .catch((e) => {
        if (alive) setError(errorText(e));
      })
      .finally(() => {
        if (alive) setLoading(false);
      });
    return () => {
      alive = false;
    };
  }, []);
  return (
    <div className="modal-backdrop">
      <section
        className="modal known-hosts-modal"
        role="dialog"
        aria-modal="true"
        aria-labelledby="known-hosts-title"
      >
        <button
          className="modal-close"
          aria-label="Close known hosts"
          onClick={onClose}
        >
          <X size={19} />
        </button>
        <h2 id="known-hosts-title">Known hosts.</h2>
        <p>
          SSH server fingerprints trusted by Harbor on this device, across all
          vaults. This list is separate from your operating system's OpenSSH
          known_hosts file.
        </p>
        <input
          aria-label="Search known hosts"
          placeholder="Search hostname, port, or fingerprint"
          value={search}
          onChange={(e) => setSearch(e.target.value)}
        />
        {error && (
          <div className="form-error" role="alert">
            {error}
          </div>
        )}
        {loading && <p role="status">Loading known hosts...</p>}
        <div className="known-hosts-list">
          {hosts
            .filter((h) =>
              `${h.host} ${h.port} ${h.fingerprint}`
                .toLowerCase()
                .includes(search.toLowerCase()),
            )
            .map((h) => (
              <article className="known-host" key={`${h.host}:${h.port}`}>
                <strong>
                  {h.host}:{h.port}
                </strong>
                <code>{h.fingerprint}</code>
              </article>
            ))}
        </div>
        {!loading && !hosts.length && (
          <p>
            No trusted servers yet. Harbor adds a fingerprint after you approve
            it when connecting.
          </p>
        )}
        <p className="field-hint">
          Changed fingerprints remain blocked. Verify changes with the server
          administrator before updating trust. Trust editing is not available in
          this view.
        </p>
      </section>
    </div>
  );
}
