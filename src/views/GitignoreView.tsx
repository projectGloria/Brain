import { useEffect, useMemo, useRef, useState } from "react";
import { FilePlus2, FolderOpen, RefreshCw, RotateCcw, Save, Search } from "lucide-react";
import { sameText, useUnsavedGuard } from "../hooks/useUnsavedGuard";
import type { GitignoreApplyResult, GitignoreTemplate, RepoStatus } from "../types";

interface GitignoreViewProps {
  template: GitignoreTemplate | null;
  templateError: string | null;
  repos: RepoStatus[];
  loadingRepos: boolean;
  onSaveTemplate: (content: string) => Promise<boolean>;
  onReloadTemplate: () => void;
  onApply: (repoPaths: string[]) => Promise<GitignoreApplyResult[]>;
  onRescan: () => void;
  onOpenFolder: (path: string) => void;
}

type Filter = "all" | "MISSING" | "DIFFERS" | "MATCHES";

const tone = (status: string) =>
  status === "MATCHES" ? "ok" : status === "DIFFERS" ? "warn" : status === "MISSING" ? "drift" : "muted";

export function GitignoreView({
  template,
  templateError,
  repos,
  loadingRepos,
  onSaveTemplate,
  onReloadTemplate,
  onApply,
  onRescan,
  onOpenFolder,
}: GitignoreViewProps) {
  const [draft, setDraft] = useState(template?.content ?? "");
  const [saving, setSaving] = useState(false);
  const [filter, setFilter] = useState<Filter>("all");
  const [query, setQuery] = useState("");
  const [applying, setApplying] = useState<Set<string>>(new Set());
  const [armed, setArmed] = useState<string | null>(null);

  const dirty = template ? !sameText(draft, template.content) : false;
  useUnsavedGuard("gitignore-draft", dirty, ".gitignore draft");

  // Re-seed from disk only when the draft still equals what was last loaded,
  // so a reload never throws away unsaved edits.
  const seeded = useRef<string | null>(template?.content ?? null);
  useEffect(() => {
    if (!template) return;
    setDraft((current) =>
      seeded.current === null || sameText(current, seeded.current) ? template.content : current
    );
    seeded.current = template.content;
  }, [template?.content, template?.path]);

  useEffect(() => {
    if (!armed) return;
    const t = window.setTimeout(() => setArmed(null), 4000);
    return () => window.clearTimeout(t);
  }, [armed]);

  const gitRepos = useMemo(
    () => repos.filter((r) => r.is_git_repo && r.status !== "ERROR"),
    [repos]
  );

  const counts = useMemo(() => {
    const c = { MISSING: 0, DIFFERS: 0, MATCHES: 0 };
    for (const r of gitRepos) {
      if (r.gitignore_status in c) c[r.gitignore_status as keyof typeof c]++;
    }
    return c;
  }, [gitRepos]);

  const visible = useMemo(() => {
    const q = query.trim().toLowerCase();
    return gitRepos.filter((r) => {
      if (filter !== "all" && r.gitignore_status !== filter) return false;
      if (q && !r.repo.toLowerCase().includes(q) && !r.repo_path.toLowerCase().includes(q)) {
        return false;
      }
      return true;
    });
  }, [gitRepos, filter, query]);

  const missingPaths = gitRepos
    .filter((r) => r.gitignore_status === "MISSING")
    .map((r) => r.repo_path);

  const canApply = !!template?.exists && !dirty;

  const runApply = async (paths: string[]) => {
    setApplying((s) => new Set([...s, ...paths]));
    try {
      await onApply(paths);
    } finally {
      setApplying((s) => {
        const next = new Set(s);
        paths.forEach((p) => next.delete(p));
        return next;
      });
    }
  };

  const handleApplyOne = (repo: RepoStatus) => {
    if (repo.gitignore_status === "DIFFERS" && armed !== repo.repo_path) {
      setArmed(repo.repo_path);
      return;
    }
    setArmed(null);
    void runApply([repo.repo_path]);
  };

  const handleSave = async () => {
    setSaving(true);
    try {
      await onSaveTemplate(draft);
    } finally {
      setSaving(false);
    }
  };

  return (
    <div style={{ display: "flex", flexDirection: "column", height: "100%", minHeight: 0 }}>
      <div className="view-header-bar">
        <div>
          <div className="page-eyebrow">REPO HYGIENE // IGNORE MATRIX</div>
          <h1 className="page-title" style={{ fontSize: 16 }}>
            Shared .gitignore
          </h1>
          <p className="page-subtitle">
            Edit one draft, then write it to any repository with one click. Applying merges:
            the draft comes first, the repo's own extra rules are kept below it, and the old file is saved as <code>.gitignore.bak</code>.
          </p>
        </div>
        <div style={{ display: "flex", gap: 8 }}>
          <button type="button" className="btn" onClick={onRescan} disabled={loadingRepos}>
            <RefreshCw size={14} className={loadingRepos ? "spin" : ""} />
            <span>Rescan</span>
          </button>
          <button
            type="button"
            className="btn btn-primary"
            onClick={() => runApply(missingPaths)}
            disabled={!canApply || missingPaths.length === 0 || applying.size > 0}
            title={!canApply ? "Save the draft first" : "Write the draft to every repo without a .gitignore"}
          >
            <FilePlus2 size={14} />
            <span>Apply to all missing ({missingPaths.length})</span>
          </button>
        </div>
      </div>

      <div className="view-split">
        <div className="editor-pane gitignore-draft">
          <div className="editor-header">
            <div style={{ minWidth: 0 }}>
              <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
                <span style={{ fontFamily: "var(--font-mono)", fontWeight: 700 }}>
                  templates/gitignore
                </span>
                {dirty && <span className="mini-tag accent">UNSAVED</span>}
                {template && !template.exists && <span className="mini-tag">NOT SAVED YET</span>}
              </div>
              <div className="cli-path selectable">{template?.path ?? "—"}</div>
            </div>
            <div style={{ display: "flex", gap: 8 }}>
              <button
                type="button"
                className="btn btn-sm"
                onClick={() => template && setDraft(template.content)}
                disabled={!dirty}
                title="Discard unsaved edits"
              >
                <RotateCcw size={12} />
              </button>
              <button
                type="button"
                className="btn btn-sm btn-primary"
                onClick={handleSave}
                disabled={!template || saving || (!dirty && template.exists)}
              >
                <Save size={12} />
                <span>{saving ? "Saving..." : "Save draft"}</span>
              </button>
            </div>
          </div>

          {templateError ? (
            <div style={{ padding: 18 }}>
              <div className="alert-strip warn selectable">{templateError}</div>
              <button type="button" className="btn btn-sm" style={{ marginTop: 10 }} onClick={onReloadTemplate}>
                Retry
              </button>
            </div>
          ) : (
            <div className="code-editor-wrap">
              <textarea
                className="code-textarea"
                value={draft}
                onChange={(e) => setDraft(e.target.value)}
                spellCheck={false}
                disabled={!template}
                aria-label=".gitignore draft"
              />
            </div>
          )}

          <div className="editor-footer">
            <span>Versioned with the Brain repo.</span>
            <span>{draft.split(/\r?\n/).length} lines</span>
          </div>
        </div>

        <div className="gitignore-repos">
          <div className="toolbar" style={{ marginBottom: 10 }}>
            <div className="toolbar-left">
              <div className="search-wrap">
                <Search size={14} />
                <input
                  type="text"
                  className="input search-input"
                  placeholder="Filter repositories..."
                  value={query}
                  onChange={(e) => setQuery(e.target.value)}
                  aria-label="Filter repositories"
                />
              </div>
            </div>
          </div>
          <div className="pill-group" role="tablist" aria-label=".gitignore status filter" style={{ marginBottom: 10 }}>
            {(
              [
                ["all", `All (${gitRepos.length})`],
                ["MISSING", `Missing (${counts.MISSING})`],
                ["DIFFERS", `Differs (${counts.DIFFERS})`],
                ["MATCHES", `Matches (${counts.MATCHES})`],
              ] as const
            ).map(([id, label]) => (
              <button
                key={id}
                type="button"
                role="tab"
                aria-selected={filter === id}
                className={`filter-pill ${filter === id ? "active" : ""}`}
                onClick={() => setFilter(id)}
              >
                {label}
              </button>
            ))}
          </div>

          {dirty && (
            <div className="alert-strip info" style={{ marginBottom: 10 }}>
              Save the draft before applying it to repositories.
            </div>
          )}

          {visible.length === 0 ? (
            <div className="empty-state">No git repositories match.</div>
          ) : (
            <div className="gitignore-repo-list">
              {visible.map((r) => {
                const busy = applying.has(r.repo_path);
                const isArmed = armed === r.repo_path;
                return (
                  <div key={r.repo_path} className="gitignore-row">
                    <div style={{ minWidth: 0 }}>
                      <div className="repo-name">{r.repo}</div>
                      <div className="repo-path selectable" title={r.repo_path}>
                        {r.repo_path}
                      </div>
                    </div>
                    <span className={`status-badge ${tone(r.gitignore_status)}`}>
                      {r.gitignore_status}
                    </span>
                    <div style={{ display: "flex", gap: 6 }}>
                      {r.gitignore_status !== "MATCHES" && (
                        <button
                          type="button"
                          className={`btn btn-sm ${isArmed ? "btn-danger" : ""}`}
                          onClick={() => handleApplyOne(r)}
                          disabled={!canApply || busy}
                          title={
                            r.gitignore_status === "DIFFERS"
                              ? "Merge the draft into the existing .gitignore (extra rules kept, backup in .gitignore.bak)"
                              : "Add the draft as .gitignore"
                          }
                        >
                          <FilePlus2 size={12} className={busy ? "spin" : ""} />
                          <span>
                            {busy
                              ? "Writing..."
                              : isArmed
                              ? "Click again to merge"
                              : r.gitignore_status === "DIFFERS"
                              ? "Merge"
                              : "Add"}
                          </span>
                        </button>
                      )}
                      <button
                        type="button"
                        className="btn btn-sm"
                        onClick={() => onOpenFolder(r.repo_path)}
                        aria-label={`Open ${r.repo} in Explorer`}
                      >
                        <FolderOpen size={12} />
                      </button>
                    </div>
                  </div>
                );
              })}
            </div>
          )}
        </div>
      </div>
    </div>
  );
}
