import { useEffect, useState } from "react";
import {
  Activity,
  Bot,
  ExternalLink,
  Eye,
  Hammer,
  Rocket,
  Square,
  TerminalSquare,
  X,
} from "lucide-react";
import { api } from "../api";

export interface BackgroundQuery {
  id: string;
  kind: "evolve" | "agent" | "cli" | "brain";
  /** Short label shown in the TitleBar when this is the only active query. */
  shortLabel: string;
  title: string;
  subtitle: string;
  lastLine: string | null;
  startedAt: number;
  repoPath?: string;
  runUrl?: string | null;
  canStop?: boolean;
  onInspect?: () => void;
  onStop?: () => void;
}

interface QueriesModalProps {
  queries: BackgroundQuery[];
  onClose: () => void;
}

function formatElapsed(startedAt: number, now: number): string {
  const sec = Math.max(0, Math.floor((now - startedAt) / 1000));
  const m = Math.floor(sec / 60);
  return m > 0 ? `${m}m ${String(sec % 60).padStart(2, "0")}s` : `${sec}s`;
}

function KindIcon({ kind }: { kind: BackgroundQuery["kind"] }) {
  if (kind === "evolve") return <Rocket size={14} className="spin" />;
  if (kind === "agent") return <Bot size={14} />;
  if (kind === "cli") return <TerminalSquare size={14} />;
  return <Hammer size={14} className="spin" />;
}

export function QueriesModal({ queries, onClose }: QueriesModalProps) {
  const [now, setNow] = useState(Date.now());

  useEffect(() => {
    const t = window.setInterval(() => setNow(Date.now()), 1000);
    return () => window.clearInterval(t);
  }, []);

  useEffect(() => {
    const onKeyDown = (e: KeyboardEvent) => {
      if (e.key === "Escape") onClose();
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [onClose]);

  return (
    <div className="release-modal-backdrop" onClick={onClose} role="presentation">
      <div
        className="release-modal"
        role="dialog"
        aria-modal="true"
        aria-labelledby="queries-modal-title"
        onClick={(e) => e.stopPropagation()}
      >
        <div className="release-modal-head">
          <div>
            <div className="page-eyebrow">BACKGROUND ACTIVITY // LIVE QUERIES</div>
            <h2 id="queries-modal-title" className="release-modal-title">
              <Activity size={16} />
              <span>Active Queries ({queries.length})</span>
            </h2>
          </div>
          <button
            type="button"
            className="icon-btn"
            onClick={onClose}
            aria-label="Close queries modal"
          >
            <X size={15} />
          </button>
        </div>

        <div className="release-modal-body">
          {queries.length === 0 ? (
            <div className="empty-state" style={{ padding: "28px 0" }}>
              All background queries have finished.
            </div>
          ) : (
            <div className="queries-list">
              {queries.map((q) => (
                <div key={q.id} className="query-card">
                  <div className="query-card-head">
                    <div style={{ minWidth: 0 }}>
                      <div className="query-card-title">
                        <KindIcon kind={q.kind} />
                        <span>{q.title}</span>
                      </div>
                      <div className="query-card-sub">
                        <span>{q.subtitle}</span>
                        <span>·</span>
                        <span>{formatElapsed(q.startedAt, now)}</span>
                      </div>
                    </div>

                    <div style={{ display: "flex", gap: 6, flexShrink: 0 }}>
                      {q.runUrl && (
                        <button
                          type="button"
                          className="btn btn-sm"
                          onClick={() => {
                            api.openUrl(q.runUrl!).catch(() => {});
                          }}
                          title="Open GitHub Actions run"
                        >
                          <ExternalLink size={12} />
                        </button>
                      )}
                      {q.onInspect && (
                        <button
                          type="button"
                          className="btn btn-sm btn-primary"
                          onClick={q.onInspect}
                        >
                          <Eye size={12} />
                          <span>Inspect</span>
                        </button>
                      )}
                      {q.canStop && q.onStop && (
                        <button
                          type="button"
                          className="btn btn-sm btn-danger"
                          onClick={q.onStop}
                          title="Stop this query"
                        >
                          <Square size={11} />
                          <span>Stop</span>
                        </button>
                      )}
                    </div>
                  </div>

                  {q.lastLine && (
                    <div className="query-last-line selectable" title={q.lastLine}>
                      {q.lastLine}
                    </div>
                  )}
                </div>
              ))}
            </div>
          )}

          <div className="release-modal-foot">
            <span className="release-meta-sub">
              Background tasks keep running while you work across tabs.
            </span>
            <button type="button" className="btn btn-primary" onClick={onClose}>
              Close
            </button>
          </div>
        </div>
      </div>
    </div>
  );
}
