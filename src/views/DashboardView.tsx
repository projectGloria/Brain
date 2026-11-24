import { useMemo, useState } from "react";
import {
  FolderOpen,
  GitBranch,
  RefreshCw,
  Search,
  Sparkles,
  CheckCircle2,
  AlertTriangle,
  FileWarning,
  XCircle,
} from "lucide-react";
import type { RepoStatus } from "../types";

interface DashboardViewProps {
  repos: RepoStatus[];
  loading: boolean;
  syncingRepo: string | null;
  syncingAll: boolean;
  onRescan: () => void;
  onRecheck: (repoPath: string) => void;
  onSyncRepo: (repoPath: string) => void;
  onSyncAllDrifted: (repoPaths: string[]) => void;
  onOpenFolder: (path: string) => void;
}

type StatusFilter = "all" | "drifted" | "ok" | "missing";

export function DashboardView({
  repos,
  loading,
  syncingRepo,
  syncingAll,
  onRescan,
  onRecheck,
  onSyncRepo,
  onSyncAllDrifted,
  onOpenFolder,
}: DashboardViewProps) {
  const [query, setQuery] = useState("");
  const [filter, setFilter] = useState<StatusFilter>("all");

  const stats = useMemo(() => {
    let upToDate = 0;
    let brainDrift = 0;
    let repoDrift = 0;
    let attention = 0;
    for (const r of repos) {
      if (r.status === "UP TO DATE") upToDate++;
      else if (r.status === "BRAIN DRIFT") brainDrift++;
      else if (r.status === "REPO DRIFT") repoDrift++;
      else attention++;
    }
    return { total: repos.length, upToDate, brainDrift, repoDrift, attention };
  }, [repos]);

  const syncablePaths = useMemo(
    () =>
      repos
        .filter(
          (r) =>
            r.status === "BRAIN DRIFT" ||
            r.status === "NO MARKER" ||
            (r.status !== "NO AGENTS.MD" && !r.has_project_brain)
        )
        .map((r) => r.repo_path),
    [repos]
  );

  const filteredRepos = useMemo(() => {
    const q = query.trim().toLowerCase();
    return repos.filter((r) => {
      if (q) {
        const matchName = r.repo.toLowerCase().includes(q);
        const matchPath = r.repo_path.toLowerCase().includes(q);
        if (!matchName && !matchPath) return false;
      }
      if (filter === "ok") return r.status === "UP TO DATE";
      if (filter === "missing") return r.status === "NO AGENTS.MD";
      if (filter === "drifted") {
        return r.status !== "UP TO DATE" && r.status !== "NO AGENTS.MD";
      }
      return true;
    });
  }, [repos, query, filter]);

  const badgeClass = (status: string) => {
    if (status === "UP TO DATE") return "ok";
    if (status === "BRAIN DRIFT") return "drift";
    if (status === "REPO DRIFT") return "warn";
    if (status === "OVER BUDGET") return "error";
    return "muted";
  };

  const StatusIcon = ({ status }: { status: string }) => {
    if (status === "UP TO DATE") return <CheckCircle2 size={12} />;
    if (status === "OVER BUDGET") return <XCircle size={12} />;
    if (status === "NO AGENTS.MD" || status === "NO MARKER")
      return <FileWarning size={12} />;
    return <AlertTriangle size={12} />;
  };

  return (
    <div className="view-scroll">
      <div className="page-header">
        <div>
          <h1 className="page-title">Repository Drift Monitor</h1>
          <p className="page-subtitle">
            Track SHA-256 brain fingerprints, 120-line AGENTS.md budgets, and
            manifest drift across workspaces.
          </p>
        </div>

        <div style={{ display: "flex", gap: 8 }}>
          <button
            type="button"
            className="btn"
            onClick={onRescan}
            disabled={loading}
          >
            <RefreshCw size={14} className={loading ? "spin" : ""} />
            <span>Rescan</span>
          </button>

          <button
            type="button"
            className="btn btn-primary"
            onClick={() => onSyncAllDrifted(syncablePaths)}
            disabled={syncingAll || syncablePaths.length === 0}
          >
            <Sparkles size={14} className={syncingAll ? "spin" : ""} />
            <span>
              {syncingAll
                ? "Syncing..."
                : `Sync Drifted (${syncablePaths.length})`}
            </span>
          </button>
        </div>
      </div>

      <div className="kpi-grid">
        <div className="kpi-card">
          <span className="kpi-label">Total Repos</span>
          <span className="kpi-value">{stats.total}</span>
        </div>
        <div className="kpi-card">
          <span className="kpi-label">Up to Date</span>
          <span className="kpi-value green">{stats.upToDate}</span>
        </div>
        <div className="kpi-card">
          <span className="kpi-label">Brain Drift</span>
          <span className="kpi-value accent">{stats.brainDrift}</span>
        </div>
        <div className="kpi-card">
          <span className="kpi-label">Repo Drift</span>
          <span className="kpi-value amber">{stats.repoDrift}</span>
        </div>
        <div className="kpi-card">
          <span className="kpi-label">Over / Missing</span>
          <span className="kpi-value red">{stats.attention}</span>
        </div>
      </div>

      <div className="toolbar">
        <div className="toolbar-left">
          <div className="search-wrap">
            <Search size={14} />
            <input
              type="text"
              className="input search-input"
              placeholder="Filter repositories..."
              value={query}
              onChange={(e) => setQuery(e.target.value)}
            />
          </div>

          <div className="pill-group" role="tablist" aria-label="Status filter">
            {(
              [
                ["all", "All"],
                ["drifted", "Needs Attention"],
                ["ok", "Up to Date"],
                ["missing", "No AGENTS.md"],
              ] as const
            ).map(([id, label]) => (
              <button
                key={id}
                type="button"
                className={`filter-pill ${filter === id ? "active" : ""}`}
                onClick={() => setFilter(id)}
              >
                {label}
              </button>
            ))}
          </div>
        </div>

        <span style={{ fontSize: 12, color: "var(--text-muted)" }}>
          Showing {filteredRepos.length} of {repos.length}
        </span>
      </div>

      {filteredRepos.length === 0 ? (
        <div className="empty-state">
          <p style={{ margin: 0, fontWeight: 600, color: "var(--text-secondary)" }}>
            No matching repositories found
          </p>
          <p style={{ margin: "6px 0 0", fontSize: 12 }}>
            Adjust your filter or add scan roots in Settings.
          </p>
        </div>
      ) : (
        <div className="repo-list">
          {filteredRepos.map((repo) => {
            const pct =
              repo.lines_count !== null
                ? Math.min(100, Math.round((repo.lines_count / 120) * 100))
                : 0;
            const budgetTone =
              repo.lines_count === null
                ? ""
                : repo.lines_count > 120
                ? "over"
                : repo.lines_count >= 100
                ? "warn"
                : "";
            const markerMismatch =
              repo.sync_marker !== "-" &&
              repo.sync_marker !== "none" &&
              repo.sync_marker !== repo.expected;
            const isSyncing = syncingRepo === repo.repo_path;

            return (
              <div key={repo.repo_path} className="repo-card">
                <div className="repo-identity">
                  <div className="repo-name-row">
                    <span className="repo-name">{repo.repo}</span>
                    {repo.is_brain_repo && (
                      <span className="mini-tag accent">MASTER REPO</span>
                    )}
                    {repo.is_git_repo && (
                      <span className="mini-tag" title="Git repository">
                        <GitBranch
                          size={10}
                          style={{ verticalAlign: "middle", marginRight: 3 }}
                        />
                        git
                      </span>
                    )}
                    {repo.has_project_brain && (
                      <span className="mini-tag">.brain/</span>
                    )}
                    {repo.has_claude_md && (
                      <span className="mini-tag">CLAUDE.md</span>
                    )}
                  </div>

                  <div className="repo-path selectable" title={repo.repo_path}>
                    {repo.repo_path}
                  </div>

                  {(repo.drifted_files.length > 0 ||
                    repo.local_playbooks.length > 0) && (
                    <div className="repo-meta-tags">
                      {repo.drifted_files.map((df) => (
                        <span
                          key={df}
                          className="mini-tag"
                          style={{
                            borderColor: "rgba(245, 158, 11, 0.35)",
                            color: "#fbbf24",
                          }}
                        >
                          changed: {df}
                        </span>
                      ))}
                      {repo.local_playbooks.map((lp) => (
                        <span key={lp} className="mini-tag accent">
                          {lp}
                        </span>
                      ))}
                    </div>
                  )}
                </div>

                <div style={{ display: "flex", flexDirection: "column", gap: 6 }}>
                  <span className={`status-badge ${badgeClass(repo.status)}`}>
                    <StatusIcon status={repo.status} />
                    <span>{repo.status}</span>
                  </span>
                  <span
                    style={{
                      fontSize: 11,
                      color: "var(--text-muted)",
                      overflow: "hidden",
                      textOverflow: "ellipsis",
                      whiteSpace: "nowrap",
                    }}
                    title={repo.message}
                  >
                    {repo.message}
                  </span>
                </div>

                <div style={{ display: "flex", flexDirection: "column", gap: 8 }}>
                  <div className="budget-col">
                    <div className="budget-top">
                      <span>AGENTS.md</span>
                      <span>{repo.lines_display}</span>
                    </div>
                    <div className="budget-bar">
                      <div
                        className={`budget-fill ${budgetTone}`}
                        style={{ width: `${pct}%` }}
                      />
                    </div>
                  </div>

                  <div className="hash-col">
                    <div className="hash-row">
                      <span>Marker</span>
                      <code className={`selectable ${markerMismatch ? "mismatch" : ""}`}>
                        {repo.sync_marker}
                      </code>
                    </div>
                    <div className="hash-row">
                      <span>Expected</span>
                      <code className="selectable">{repo.expected}</code>
                    </div>
                  </div>
                </div>

                <div className="repo-actions">
                  <button
                    type="button"
                    className="btn btn-sm"
                    onClick={() => onSyncRepo(repo.repo_path)}
                    disabled={isSyncing}
                    title="Sync ~/.brain/*.md into <repo>/.brain/ and update .git/info/exclude"
                  >
                    <RefreshCw size={12} className={isSyncing ? "spin" : ""} />
                    <span>Sync .brain/</span>
                  </button>
                  <button
                    type="button"
                    className="btn btn-sm"
                    onClick={() => onRecheck(repo.repo_path)}
                    title="Re-run drift check"
                  >
                    Recheck
                  </button>
                  <button
                    type="button"
                    className="btn btn-sm"
                    onClick={() => onOpenFolder(repo.repo_path)}
                    title="Open folder in Explorer"
                  >
                    <FolderOpen size={13} />
                  </button>
                </div>
              </div>
            );
          })}
        </div>
      )}
    </div>
  );
}
