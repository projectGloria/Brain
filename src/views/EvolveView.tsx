import { useMemo, useState } from "react";
import { ExternalLink, FolderOpen, RefreshCw, Rocket, Search } from "lucide-react";
import { api } from "../api";
import { HoverTip } from "../components/HoverTip";
import { ReleaseModal } from "../components/ReleaseModal";
import type { ReleaseRuns } from "../hooks/useReleaseRuns";
import type { DnaProjectSummary, ReleaseJob } from "../types";

interface EvolveViewProps {
  projects: DnaProjectSummary[];
  loading: boolean;
  releaseRuns: ReleaseRuns;
  onRescan: () => void;
  onOpenFolder: (path: string) => void;
}

type Filter = "all" | "ready" | "active";

function jobTone(status: ReleaseJob["status"]) {
  if (status === "done") return "ok";
  if (status === "running") return "drift";
  if (status === "failed") return "error";
  return "muted";
}

function jobLabel(job: ReleaseJob) {
  if (job.status === "running") return `RELEASING v${job.new_version}`;
  if (job.status === "done") return `LIVE v${job.new_version}`;
  if (job.status === "failed") return `FAILED v${job.new_version}`;
  return `STOPPED v${job.new_version}`;
}

export function EvolveView({
  projects,
  loading,
  releaseRuns,
  onRescan,
  onOpenFolder,
}: EvolveViewProps) {
  const [filter, setFilter] = useState<Filter>("all");
  const [query, setQuery] = useState("");
  const [releaseRepoPath, setReleaseRepoPath] = useState<string | null>(null);

  const counts = useMemo(() => {
    let ready = 0;
    let active = 0;
    for (const p of projects) {
      if (p.has_gloria_toml) ready++;
      if (releaseRuns.jobsByRepo[p.repo_path]) active++;
    }
    return { ready, active };
  }, [projects, releaseRuns.jobsByRepo]);

  const visible = useMemo(() => {
    const q = query.trim().toLowerCase();
    return projects.filter((p) => {
      if (filter === "ready" && !p.has_gloria_toml) return false;
      if (filter === "active" && !releaseRuns.jobsByRepo[p.repo_path]) return false;
      if (
        q &&
        !p.repo.toLowerCase().includes(q) &&
        !p.repo_path.toLowerCase().includes(q) &&
        !p.stacks.some((s) => s.toLowerCase().includes(q))
      ) {
        return false;
      }
      return true;
    });
  }, [projects, filter, query, releaseRuns.jobsByRepo]);

  const releaseTargetName = releaseRepoPath
    ? projects.find((p) => p.repo_path === releaseRepoPath)?.repo ??
      releaseRepoPath.replace(/[\\/]+$/, "").split(/[\\/]/).pop() ??
      releaseRepoPath
    : "";

  return (
    <div className="view-scroll">
      <div className="page-header">
        <div>
          <div className="page-eyebrow">RELEASE PIPELINE // GLORIA-GITHUB-RELEASE</div>
          <h1 className="page-title">Evolve</h1>
          <p className="page-subtitle">
            Bump every version manifest, validate <code>gloria.toml</code>, commit &amp; push to{" "}
            <code>main</code>, and watch GitHub Actions publish the release — with zero AI tokens.
          </p>
        </div>
        <div style={{ display: "flex", gap: 8 }}>
          <button type="button" className="btn" onClick={onRescan} disabled={loading}>
            <RefreshCw size={14} className={loading ? "spin" : ""} />
            <span>Rescan</span>
          </button>
        </div>
      </div>

      <div className="toolbar">
        <div className="toolbar-left">
          <div className="search-wrap">
            <Search size={14} />
            <input
              type="text"
              className="input search-input"
              placeholder="Filter repositories or stacks..."
              value={query}
              onChange={(e) => setQuery(e.target.value)}
              aria-label="Filter repositories"
            />
          </div>
          <div className="pill-group" role="tablist" aria-label="Evolve filter">
            {(
              [
                ["all", `All (${projects.length})`],
                ["ready", `gloria.toml (${counts.ready})`],
                ["active", `Jobs (${counts.active})`],
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
        </div>
      </div>

      {visible.length === 0 ? (
        <div className="empty-state">No repositories match.</div>
      ) : (
        <div className="evolve-list">
          {visible.map((p) => {
            const job = releaseRuns.jobsByRepo[p.repo_path] ?? null;
            const isRunning = job?.status === "running";
            return (
              <div
                key={p.repo_path}
                className={`dna-project-row evolve-row ${job ? "has-job" : ""}`}
                onClick={() => setReleaseRepoPath(p.repo_path)}
              >
                <div style={{ minWidth: 0 }}>
                  <div style={{ display: "flex", alignItems: "center", gap: 8, flexWrap: "wrap" }}>
                    <span className="repo-name">{p.repo}</span>
                    {p.has_gloria_toml && (
                      <HoverTip text="Has a gloria.toml manifest for GloriaSetup Windows installer packaging.">
                        <span className="status-badge ok">gloria.toml</span>
                      </HoverTip>
                    )}
                    {job && (
                      <HoverTip
                        text={
                          job.status === "running"
                            ? "Release pipeline is running in the background."
                            : job.status === "done"
                            ? "GitHub Release was published successfully."
                            : job.status === "failed"
                            ? job.error ?? "Release pipeline failed. Click to inspect."
                            : "Release was stopped before completion."
                        }
                      >
                        <span className={`status-badge ${jobTone(job.status)}`}>
                          {isRunning && <RefreshCw size={10} className="spin" />}
                          {jobLabel(job)}
                        </span>
                      </HoverTip>
                    )}
                    {p.manifests.length > 0 && (
                      <span className="mini-tag">{p.manifests.length} manifest(s)</span>
                    )}
                  </div>
                  <div className="repo-path selectable" title={p.repo_path}>
                    {p.repo_path}
                  </div>
                  <div className="dna-stack-row">
                    {p.stacks.map((s) => (
                      <span key={s} className="dna-stack-pill">
                        {s}
                      </span>
                    ))}
                  </div>
                </div>

                <div
                  style={{ display: "flex", gap: 6, flexShrink: 0, alignItems: "center" }}
                  onClick={(e) => e.stopPropagation()}
                >
                  {job?.release_url && (
                    <button
                      type="button"
                      className="btn btn-sm"
                      onClick={() => {
                        api.openUrl(job.release_url!).catch(() => {});
                      }}
                      title="Open GitHub Release"
                    >
                      <ExternalLink size={12} />
                      <span>Release</span>
                    </button>
                  )}
                  <button
                    type="button"
                    className={`btn btn-sm ${isRunning || p.has_gloria_toml ? "btn-primary" : ""}`}
                    onClick={() => setReleaseRepoPath(p.repo_path)}
                  >
                    <Rocket size={12} className={isRunning ? "spin" : ""} />
                    <span>
                      {isRunning
                        ? "Releasing..."
                        : job
                        ? "View release"
                        : "Release new version"}
                    </span>
                  </button>
                  <button
                    type="button"
                    className="btn btn-sm"
                    onClick={() => onOpenFolder(p.repo_path)}
                    aria-label={`Open ${p.repo} in Explorer`}
                  >
                    <FolderOpen size={12} />
                  </button>
                </div>
              </div>
            );
          })}
        </div>
      )}

      {releaseRepoPath && (
        <ReleaseModal
          repoPath={releaseRepoPath}
          repoName={releaseTargetName}
          runs={releaseRuns}
          onClose={() => setReleaseRepoPath(null)}
        />
      )}
    </div>
  );
}
