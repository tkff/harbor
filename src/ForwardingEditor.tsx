import { ArrowRight, Network, Plus, Trash2 } from "lucide-react";
import type { ForwardRule } from "./api";

export default function ForwardingEditor({
  rules,
  onChange,
}: {
  rules: ForwardRule[];
  onChange: (rules: ForwardRule[]) => void;
}) {
  const update = (id: string, patch: Partial<ForwardRule>) =>
    onChange(rules.map((r) => (r.id === id ? { ...r, ...patch } : r)));
  return (
    <section className="forward-editor" aria-label="Port forwarding rules">
      <div className="credentials-heading">
        <Network size={14} />
        <span>PORT FORWARDING</span>
        <span className="secure-label">Saved with connection</span>
      </div>
      <p className="field-hint">
        Route TCP connections through this SSH server. Rules apply to new
        sessions.
      </p>
      {rules.map((r, index) => (
        <div className="forward-rule-editor" key={r.id}>
          <div className="forward-rule-heading">
            <strong>Rule {index + 1}</strong>
            <select
              aria-label={`Rule ${index + 1} direction`}
              value={r.direction}
              onChange={(e) =>
                update(r.id, {
                  direction: e.target.value as ForwardRule["direction"],
                  bindPort:
                    e.target.value === "dynamic" && r.bindPort === 8080
                      ? 1080
                      : r.bindPort,
                  targetHost: r.targetHost || "127.0.0.1",
                  targetPort: r.targetPort || 80,
                })
              }
            >
              <option value="local">Local forwarding</option>
              <option value="remote">Remote forwarding</option>
              <option value="dynamic">Dynamic / SOCKS5 proxy</option>
            </select>
            <button
              type="button"
              className="icon-button danger-hover"
              aria-label={`Remove forwarding rule ${index + 1}`}
              onClick={() => onChange(rules.filter((x) => x.id !== r.id))}
            >
              <Trash2 size={15} />
            </button>
          </div>
          <p className="forward-explanation">
            {r.direction === "dynamic"
              ? "Local SOCKS5 proxy: applications choose destinations reached through the SSH server."
              : r.direction === "local"
                ? "Listen on this computer → destination reached from the SSH server."
                : "Listen on the SSH server → destination reached from this computer."}
          </p>
          <div className="forward-fields">
            <div>
              <label htmlFor={`${r.id}-bind`}>
                {r.direction === "remote" ? "Server" : "Local"} listen address
              </label>
              <input
                id={`${r.id}-bind`}
                value={r.bindAddress}
                placeholder="127.0.0.1"
                onChange={(e) => update(r.id, { bindAddress: e.target.value })}
                required
              />
            </div>
            <div>
              <label htmlFor={`${r.id}-port`}>Listen port</label>
              <input
                id={`${r.id}-port`}
                type="number"
                min="1"
                max="65535"
                value={r.bindPort || ""}
                onChange={(e) =>
                  update(r.id, { bindPort: Number(e.target.value) })
                }
                required
              />
            </div>
            {r.direction !== "dynamic" && (
              <>
                <div>
                  <label htmlFor={`${r.id}-target`}>Destination host</label>
                  <input
                    id={`${r.id}-target`}
                    value={r.targetHost}
                    placeholder="127.0.0.1"
                    onChange={(e) =>
                      update(r.id, { targetHost: e.target.value })
                    }
                    required
                  />
                </div>
                <div>
                  <label htmlFor={`${r.id}-target-port`}>
                    Destination port
                  </label>
                  <input
                    id={`${r.id}-target-port`}
                    type="number"
                    min="1"
                    max="65535"
                    value={r.targetPort || ""}
                    onChange={(e) =>
                      update(r.id, { targetPort: Number(e.target.value) })
                    }
                    required
                  />
                </div>
              </>
            )}
          </div>
          {r.direction !== "dynamic" &&
            r.bindAddress &&
            !["127.0.0.1", "::1", "localhost"].includes(r.bindAddress) && (
              <p className="field-hint forward-exposure">
                This listen address may allow connections from other computers.
                Server and network settings also apply.
              </p>
            )}
          <label className="checkbox-label">
            <input
              type="checkbox"
              checked={r.autoStart}
              onChange={(e) => update(r.id, { autoStart: e.target.checked })}
            />
            Start automatically with the SSH session
          </label>
        </div>
      ))}
      <button
        type="button"
        className="secondary full-width"
        disabled={rules.length >= 32}
        onClick={() =>
          onChange([
            ...rules,
            {
              id: crypto.randomUUID(),
              direction: "local",
              bindAddress: "127.0.0.1",
              bindPort: 8080,
              targetHost: "127.0.0.1",
              targetPort: 80,
              autoStart: true,
            },
          ])
        }
      >
        <Plus size={14} />
        Add forwarding rule
        <ArrowRight size={13} />
      </button>
      <p className="field-hint">
        SOCKS5 proxies listen only on this computer's loopback addresses and do
        not require a proxy password. TCP CONNECT only; UDP is not supported.
        Configure your application to send hostnames through the proxy for
        remote DNS.
      </p>
    </section>
  );
}
