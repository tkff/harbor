import { ArrowRight, Network, X } from "lucide-react";
import type { ForwardRule, ForwardStatus } from "./api";
type Props = {
  rules: ForwardRule[];
  states: Record<string, ForwardStatus>;
  pending: Record<string, boolean>;
  connected: boolean;
  onToggle: (id: string, start: boolean) => void;
  onClose: () => void;
};
export default function ForwardingPanel({
  rules,
  states,
  pending,
  connected,
  onToggle,
  onClose,
}: Props) {
  return (
    <aside className="forward-panel" aria-label="Session port forwarding">
      <div className="forward-panel-heading">
        <h3>Port forwarding</h3>
        <button
          className="icon-button"
          aria-label="Close forwarding panel"
          onClick={() => onClose()}
        >
          <X size={16} />
        </button>
      </div>
      <p>
        Rules belong to this SSH session. Closing the tab or locking the vault
        stops them.
      </p>
      {!rules.length && (
        <div className="forward-empty">
          No rules saved. Edit the connection, add a local, remote, or SOCKS5
          forwarding rule, then reconnect.
        </div>
      )}
      {rules.map((rule) => {
        const state = states[rule.id] ?? {
          state: "inactive",
          message: null,
        };
        const running = state.state === "active";
        return (
          <div className="forward-runtime-rule" key={rule.id}>
            <div className="forward-runtime-heading">
              <strong>
                {rule.direction === "dynamic"
                  ? "SOCKS5 proxy"
                  : rule.direction === "local"
                    ? "Local"
                    : "Remote"}
              </strong>
              <span className={`forward-state ${state.state}`}>
                {state.state}
              </span>
            </div>
            <div className="forward-endpoints">
              <code>
                {rule.bindAddress}:{rule.bindPort}
              </code>
              <ArrowRight size={13} />
              <code>
                {rule.direction === "dynamic"
                  ? "Client-selected destination"
                  : `${rule.targetHost}:${rule.targetPort}`}
              </code>
            </div>
            <p>
              {rule.direction === "dynamic"
                ? "Local SOCKS5 - TCP only, no proxy password. Hostnames are resolved through SSH."
                : rule.direction === "local"
                  ? "This computer → SSH server destination"
                  : "SSH server → this computer’s destination"}
            </p>
            {state.message && (
              <div className="forward-runtime-error" role="status">
                {state.message}
              </div>
            )}
            <button
              className="secondary full-width"
              disabled={
                !connected || pending[rule.id] || state.state === "starting"
              }
              onClick={() => void onToggle(rule.id, !running)}
            >
              {pending[rule.id]
                ? "Updating…"
                : running
                  ? "Stop forwarding"
                  : "Start forwarding"}
            </button>
          </div>
        );
      })}
      <div className="forward-future">
        <Network size={15} />
        <div>
          <strong>SOCKS5 application setup</strong>
          <p>
            Use the proxy listen address and port in your application. Enable
            proxy DNS or use socks5h to resolve names through SSH. UDP is not
            supported.
          </p>
        </div>
      </div>
    </aside>
  );
}
