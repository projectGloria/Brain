import { useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
import {
  AlertTriangle,
  Check,
  ExternalLink,
  GitBranch,
  RefreshCw,
  Rocket,
  Square,
  X,
} from "lucide-react";
import { api } from "../api";
import type { ReleaseRuns } from "../hooks/useReleaseRuns";
import type { ReleasePreflight, ReleaseStepStatus } from "../types";

interface ReleaseModalProps {
  repoPath: string;
  repoName: string;
  runs: ReleaseRuns;
  onClose: () => void;
  onReleaseSucceeded?: (repoPath: string) => void;
}

const SEMVER_RE = /^[0-9]+\.[0-9]+\.[0-9]+(?:-[0-9A-Za-z.-]+)?$/;

function normalizeVer(raw: string): string {
  return raw.trim().replace(/^[vV]+/, "").trim();
}

function stepIcon(status: ReleaseStepStatus) {
  if (status === "running") return <RefreshCw size={11} className="spin" />;
  if (status === "done") return <Check size={11} />;
  if (status === "failed") return <X size={11} />;
  return <span className="release-step-dot" />;
}

export function ReleaseModal({
  repoPath,
  repoName,
  runs,
  onClose,
  onReleaseSucceeded,
}: ReleaseModalProps) {
  const job = runs.jobsByRepo[repoPath] ?? null;

  const [preflight, setPreflight] = useState<ReleasePreflight | null>(null);
  const [preflightLoading, setPreflightLoading] = useState(false);
  const [preflightError, setPreflightError] = useState<string | null>(null);
  const [versionInput, setVersionInput] = useState("");
  const [noteInput, setNoteInput] = useState("");
  const [submitting, setSubmitting] = useState(false);

  const inputRef = useRef<HTMLInputElement>(null);
  const logRef = useRef<HTMLDivElement>(null);
  const stick = useRef(true);
  const notifiedSuccessRef = useRef<string | null>(null);

  useEffect(() => {
    if (job) return;
    let cancelled = false;
    setPreflightLoading(true);
    setPreflightError(null);
    api
      .releasePreflight(repoPath)
      .then((res) => {
        if (cancelled) return;
        setPreflight(res);
      })
      .catch((err) => {
        if (cancelled) return;
        setPreflightError(String(err));
      })
      .finally(() => {
        if (!cancelled) setPreflightLoading(false);
      });
    return () => {
      cancelled = true;
    };
  }, [repoPath, job]);

  useEffect(() => {
    if (!job && !preflightLoading) {
      inputRef.current?.focus();
    }
  }, [job, preflightLoading]);

  useEffect(() => {
    const onKeyDown = (e: KeyboardEvent) => {
      if (e.key === "Escape") onClose();
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [onClose]);

  useLayoutEffect(() => {
    const el = logRef.current;
    if (el && stick.current) el.scrollTop = el.scrollHeight;
  }, [job?.lines.length]);

  useEffect(() => {
    if (job && job.status === "done" && notifiedSuccessRef.current !== job.run_id) {
      notifiedSuccessRef.current = job.run_id;
      onReleaseSucceeded?.(repoPath);
    }
  }, [job, repoPath, onReleaseSucceeded]);

  const normalized = normalizeVer(versionInput);
  const duplicateTag = useMemo(() => {
    if (!preflight || !normalized) return false;
    const tag = `v${normalized}`;
    return preflight.existing_tags.some(
      (t) => t.toLowerCase() === tag.toLowerCase() || t.toLowerCase() === normalized.toLowerCase()
    );
  }, [preflight, normalized]);

  const validationError = useMemo(() => {
    if (!versionInput.trim()) return null;
    if (!SEMVER_RE.test(normalized)) {
      return "Use semver like 1.2.0 or 1.2.0-alpha.3";
    }
    if (duplicateTag) {
      return `Release v${normalized} already exists on GitHub.`;
    }
    return null;
  }, [versionInput, normalized, duplicateTag]);

  const canSubmit =
    !preflightLoading &&
    !preflightError &&
    !!preflight &&
    preflight.blockers.length === 0 &&
    !!normalized &&
    !validationError &&
    !submitting;

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!canSubmit || !preflight) return;
    setSubmitting(true);
    try {
      await runs.start(repoPath, normalized, preflight.current_version, noteInput);
    } finally {
      setSubmitting(false);
    }
  };

  const openExternal = (url: string) => {
    api.openUrl(url).catch(() => {});
  };

  const mismatchedFiles = preflight?.version_files.filter((f) => !f.matches_primary) ?? [];

  return (
    <div className="release-modal-backdrop" onClick={onClose} role="presentation">
      <div
        className="release-modal"
        role="dialog"
        aria-modal="true"
        aria-labelledby="release-modal-title"
        onClick={(e) => e.stopPropagation()}
      >
        <div className="release-modal-head">
          <div>
            <div className="page-eyebrow">GLORIA-GITHUB-RELEASE // ZERO-TOKEN RUNNER</div>
            <h2 id="release-modal-title" className="release-modal-title">
              <Rocket size={16} />
              <span>Release {repoName}</span>
            </h2>
          </div>
          <button
            type="button"
            className="icon-btn"
            onClick={onClose}
            aria-label="Close release modal"
            title={job?.status === "running" ? "Run in background" : "Close"}
          >
            <X size={15} />
          </button>
        </div>

        {!job ? (
          <form className="release-modal-body" onSubmit={(e) => void handleSubmit(e)}>
            {preflightLoading ? (
              <div className="empty-state" style={{ padding: "28px 0" }}>
                Checking manifests, git branch, and GitHub releases...
              </div>
            ) : preflightError ? (
              <div className="alert-strip warn selectable">
                <AlertTriangle size={14} />
                <span>{preflightError}</span>
              </div>
            ) : preflight ? (
              <>
                <div className="release-preflight-grid">
                  <div className="release-meta-card">
                    <span className="kpi-label">Current Version</span>
                    <span className="release-meta-value selectable">
                      {preflight.current_version ?? "none detected"}
                    </span>
                    <span className="release-meta-sub">
                      {preflight.version_files.length} manifest(s)
                      {mismatchedFiles.length > 0
                        ? ` · ${mismatchedFiles.length} mismatch`
                        : ""}
                    </span>
                  </div>

                  <div className="release-meta-card">
                    <span className="kpi-label">Last Release</span>
                    <span className="release-meta-value selectable">
                      {preflight.last_release ?? "none"}
                    </span>
                    <span className="release-meta-sub">GitHub Releases</span>
                  </div>

                  <div className="release-meta-card">
                    <span className="kpi-label">Git Branch</span>
                    <span className="release-meta-value">
                      <GitBranch size={12} /> {preflight.git_branch ?? "detached"}
                    </span>
                    <span className="release-meta-sub">
                      {preflight.dirty_files.length === 0
                        ? "Working tree clean"
                        : `${preflight.dirty_files.length} pending change(s)`}
                    </span>
                  </div>

                  <div className="release-meta-card">
                    <span className="kpi-label">Workflow</span>
                    <span className="release-meta-value">
                      {preflight.workflow_status === "ready"
                        ? "GloriaSetup Template"
                        : preflight.workflow_status === "custom"
                        ? "Custom release.yml"
                        : "Missing"}
                    </span>
                    <span className="release-meta-sub">
                      {preflight.has_gloria_toml
                        ? "gloria.toml check enabled"
                        : preflight.is_gloria_setup
                        ? "GloriaSetup repo"
                        : "No gloria.toml"}
                    </span>
                  </div>
                </div>

                {mismatchedFiles.length > 0 && (
                  <div className="alert-strip warn">
                    <AlertTriangle size={14} />
                    <span>
                      Version mismatch:{" "}
                      {mismatchedFiles.map((f) => `${f.path} (${f.version})`).join(", ")} — all
                      files will be synced to the new version.
                    </span>
                  </div>
                )}

                {preflight.dev_self_release && (
                  <div className="alert-strip warn">
                    <AlertTriangle size={14} />
                    <span>
                      Running in <code>tauri dev</code> mode: bumping Brain&apos;s own{" "}
                      <code>Cargo.toml</code> / <code>tauri.conf.json</code> will trigger{" "}
                      <code>tauri dev</code> to kill and restart the app mid-release. Use the built
                      app (<code>gloriaTools/run.bat</code>) to release Brain itself.
                    </span>
                  </div>
                )}

                {preflight.blockers.length > 0 && (
                  <div className="alert-strip warn">
                    <AlertTriangle size={14} />
                    <span>{preflight.blockers.join(" ")}</span>
                  </div>
                )}

                <div className="release-form-row">
                  <div className="release-field-head">
                    <label htmlFor="release-version-input" className="run-label">
                      What should the new version be?
                    </label>
                    <button
                      type="button"
                      className="filter-pill"
                      onClick={() => setVersionInput(preflight.suggested_version)}
                      title="Use suggested next version"
                    >
                      Use {preflight.suggested_version}
                    </button>
                  </div>
                  <input
                    id="release-version-input"
                    ref={inputRef}
                    type="text"
                    className="input release-version-input"
                    placeholder={`e.g. ${preflight.suggested_version}`}
                    value={versionInput}
                    onChange={(e) => setVersionInput(e.target.value)}
                    disabled={preflight.blockers.length > 0 || submitting}
                    autoComplete="off"
                  />
                  {validationError && <div className="run-note warn">{validationError}</div>}
                </div>

                <div className="release-form-row">
                  <label htmlFor="release-note-input" className="run-label">
                    Release note (optional, used in commit message)
                  </label>
                  <input
                    id="release-note-input"
                    type="text"
                    className="input"
                    placeholder="e.g. Add native DNA release runner"
                    value={noteInput}
                    onChange={(e) => setNoteInput(e.target.value)}
                    disabled={preflight.blockers.length > 0 || submitting}
                    autoComplete="off"
                  />
                </div>
              </>
            ) : null}

            <div className="release-modal-foot">
              <span className="release-meta-sub">
                Bumps manifests, validates gloria.toml, commits &amp; pushes main, then watches CI.
              </span>
              <div style={{ display: "flex", gap: 8 }}>
                <button type="button" className="btn" onClick={onClose}>
                  Cancel
                </button>
                <button type="submit" className="btn btn-primary" disabled={!canSubmit}>
                  <Rocket size={13} />
                  <span>{submitting ? "Starting..." : "Release"}</span>
                </button>
              </div>
            </div>
          </form>
        ) : (
          <div className="release-modal-body">
            <div className="release-stepper" role="list" aria-label="Release pipeline">
              {job.steps.map((s) => (
                <div
                  key={s.id}
                  role="listitem"
                  className={`release-step-pill ${s.status}`}
                  title={s.detail ?? s.label}
                >
                  {stepIcon(s.status)}
                  <span>{s.label}</span>
                </div>
              ))}
            </div>

            {job.status === "done" && (
              <div className="release-success-banner">
                <div>
                  <strong>
                    {job.old_version ? `${job.old_version} → ${job.new_version}` : `v${job.new_version}`}
                  </strong>
                  <span className="release-meta-sub" style={{ marginLeft: 8 }}>
                    GitHub Release is live. Use Refresh in GloriaLauncher to see it sooner.
                  </span>
                </div>
                {job.assets.length > 0 && (
                  <div className="release-assets">
                    {job.assets.map((a) => (
                      <span key={a} className="mini-tag selectable">
                        {a}
                      </span>
                    ))}
                  </div>
                )}
              </div>
            )}

            {job.status === "failed" && job.error && (
              <div className="alert-strip warn selectable">
                <AlertTriangle size={14} />
                <span>{job.error}</span>
              </div>
            )}

            <div
              ref={logRef}
              className="run-log release-log selectable"
              onScroll={(e) => {
                const el = e.currentTarget;
                stick.current = el.scrollHeight - el.scrollTop - el.clientHeight < 40;
              }}
              role="log"
              aria-live="polite"
            >
              {job.dropped > 0 && (
                <div className="run-line info">… {job.dropped} earlier lines hidden</div>
              )}
              {job.lines.map((l) => (
                <div key={l.id} className={`run-line ${l.stream}`}>
                  {l.text}
                </div>
              ))}
              {job.status === "running" && job.lines.length === 0 && (
                <div className="run-line info">Starting release v{job.new_version}…</div>
              )}
            </div>

            <div className="release-modal-foot">
              <div style={{ display: "flex", gap: 8, flexWrap: "wrap" }}>
                {job.run_url && (
                  <button
                    type="button"
                    className="btn btn-sm"
                    onClick={() => openExternal(job.run_url!)}
                  >
                    <ExternalLink size={12} />
                    <span>Actions Run</span>
                  </button>
                )}
                {job.release_url && (
                  <button
                    type="button"
                    className="btn btn-sm btn-primary"
                    onClick={() => openExternal(job.release_url!)}
                  >
                    <ExternalLink size={12} />
                    <span>GitHub Release</span>
                  </button>
                )}
              </div>

              <div style={{ display: "flex", gap: 8 }}>
                {job.status === "running" ? (
                  <>
                    <button type="button" className="btn" onClick={onClose}>
                      Run in Background
                    </button>
                    <button
                      type="button"
                      className="btn btn-danger"
                      onClick={() => void runs.cancel(repoPath)}
                    >
                      <Square size={12} />
                      <span>Stop</span>
                    </button>
                  </>
                ) : (
                  <>
                    <button
                      type="button"
                      className="btn"
                      onClick={() => runs.clear(repoPath)}
                    >
                      New Release
                    </button>
                    <button type="button" className="btn btn-primary" onClick={onClose}>
                      Close
                    </button>
                  </>
                )}
              </div>
            </div>
          </div>
        )}
      </div>
    </div>
  );
}
