import { useEffect, useMemo, useState } from "react";
import {
  CheckSquare,
  Dna,
  Eye,
  FolderOpen,
  RefreshCw,
  Search,
  Square,
  Wrench,
} from "lucide-react";
import { api } from "../api";
import { HoverTip } from "../components/HoverTip";
import type {
  DnaApplyResult,
  DnaProjectInspect,
  DnaProjectSummary,
} from "../types";

const DNA_STATUS_HELP: Record<string, string> = {
  READY: "All expected gloriaTools/*.bat scripts exist and match Brain's latest templates.",
  OUTDATED: "One or more gloriaTools/*.bat scripts differ from Brain's latest templates.",
  PARTIAL: "Some gloriaTools/*.bat scripts exist, but others are missing.",
  MISSING: "No gloriaTools/ scripts have been generated for this repo yet.",
};

const TOOL_STATUS_HELP: Record<string, string> = {
  IN_SYNC: "Script on disk matches Brain's generated template.",
  OUTDATED: "Script on disk differs from Brain's updated template.",
  NEW: "Script does not exist yet and will be created.",
  CUSTOM: "Custom script not managed by Brain — preserved and never overwritten.",
};

interface DnaViewProps {
  projects: DnaProjectSummary[];
  loading: boolean;
  onRescan: () => void;
  onCreateTools: (repoPath: string, toolNames?: string[]) => Promise<DnaApplyResult | null>;
  onCreateMissing: (repoPaths: string[]) => Promise<void>;
  onOpenFolder: (path: string) => void;
}

type Filter = "all" | "MISSING" | "OUTDATED" | "READY";

const statusTone = (status: string) =>
  status === "READY"
    ? "ok"
    : status === "OUTDATED" || status === "PARTIAL"
    ? "warn"
    : status === "MISSING"
    ? "drift"
    : "muted";

const toolTone = (status: string) =>
  status === "IN_SYNC"
    ? "ok"
    : status === "OUTDATED"
    ? "warn"
    : status === "NEW"
    ? "drift"
    : "muted";

export function DnaView({
  projects,
  loading,
  onRescan,
  onCreateTools,
  onCreateMissing,
  onOpenFolder,
}: DnaViewProps) {
  const [filter, setFilter] = useState<Filter>("all");
  const [query, setQuery] = useState("");
  const [busyRepos, setBusyRepos] = useState<Set<string>>(new Set());
  const [batchBusy, setBatchBusy] = useState(false);

  const [selectedPath, setSelectedPath] = useState<string | null>(null);
  const [inspectData, setInspectData] = useState<DnaProjectInspect | null>(null);
  const [inspectLoading, setInspectLoading] = useState(false);
  const [activeToolName, setActiveToolName] = useState<string | null>(null);
  const [checkedTools, setCheckedTools] = useState<Set<string>>(new Set());

  useEffect(() => {
    if (!selectedPath && projects.length > 0) {
      setSelectedPath(projects[0].repo_path);
    }
  }, [projects, selectedPath]);

  const loadInspect = async (repoPath: string) => {
    setInspectLoading(true);
    try {
      const data = await api.dnaInspectRepo(repoPath);
      setInspectData(data);
      const selectable = data.tools.filter((t) => t.status !== "CUSTOM").map((t) => t.file_name);
      setCheckedTools(new Set(selectable));
      setActiveToolName((prev) =>
        prev && data.tools.some((t) => t.file_name === prev)
          ? prev
          : data.tools[0]?.file_name ?? null
      );
    } finally {
      setInspectLoading(false);
    }
  };

  useEffect(() => {
    if (selectedPath) {
      void loadInspect(selectedPath);
    } else {
      setInspectData(null);
    }
  }, [selectedPath]);

  const counts = useMemo(() => {
    const c = { MISSING: 0, OUTDATED: 0, READY: 0 };
    for (const p of projects) {
      if (p.status === "MISSING") c.MISSING++;
      else if (p.status === "READY") c.READY++;
      else c.OUTDATED++;
    }
    return c;
  }, [projects]);

  const visible = useMemo(() => {
    const q = query.trim().toLowerCase();
    return projects.filter((p) => {
      if (filter === "MISSING" && p.status !== "MISSING") return false;
      if (filter === "READY" && p.status !== "READY") return false;
      if (filter === "OUTDATED" && (p.status === "MISSING" || p.status === "READY")) {
        return false;
      }
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
  }, [projects, filter, query]);

  const missingPaths = useMemo(
    () => projects.filter((p) => p.status === "MISSING").map((p) => p.repo_path),
    [projects]
  );

  const handleRunCreate = async (repoPath: string, toolNames?: string[]) => {
    setBusyRepos((s) => new Set(s).add(repoPath));
    try {
      const res = await onCreateTools(repoPath, toolNames);
      if (res?.ok && selectedPath === repoPath) {
        await loadInspect(repoPath);
      }
    } finally {
      setBusyRepos((s) => {
        const next = new Set(s);
        next.delete(repoPath);
        return next;
      });
    }
  };

  const handleCreateAllMissing = async () => {
    if (missingPaths.length === 0) return;
    setBatchBusy(true);
    try {
      await onCreateMissing(missingPaths);
      if (selectedPath) {
        await loadInspect(selectedPath);
      }
    } finally {
      setBatchBusy(false);
    }
  };

  const toggleToolCheck = (fileName: string) => {
    setCheckedTools((prev) => {
      const next = new Set(prev);
      if (next.has(fileName)) next.delete(fileName);
      else next.add(fileName);
      return next;
    });
  };

  const activeTool =
    inspectData?.tools.find((t) => t.file_name === activeToolName) ??
    inspectData?.tools[0] ??
    null;

  return (
    <div style={{ display: "flex", flexDirection: "column", height: "100%", minHeight: 0 }}>
      <div className="view-header-bar">
        <div>
          <div className="page-eyebrow">PROJECT SCAFFOLD // GLORIATOOLS</div>
          <h1 className="page-title" style={{ fontSize: 16 }}>
            DNA
          </h1>
          <p className="page-subtitle">
            Analyze every repository's stack, ports, and manifests, then generate tailored{" "}
            <code>gloriaTools/*.bat</code> scripts in one click.
          </p>
        </div>
        <div style={{ display: "flex", gap: 8 }}>
          <button type="button" className="btn" onClick={onRescan} disabled={loading || batchBusy}>
            <RefreshCw size={14} className={loading ? "spin" : ""} />
            <span>Rescan</span>
          </button>
          <button
            type="button"
            className="btn btn-primary"
            onClick={() => void handleCreateAllMissing()}
            disabled={missingPaths.length === 0 || batchBusy}
            title="Create gloriaTools/ for every project that does not have them yet"
          >
            <Dna size={14} className={batchBusy ? "spin" : ""} />
            <span>
              {batchBusy ? "Creating..." : `Create for all missing (${missingPaths.length})`}
            </span>
          </button>
        </div>
      </div>

      <div className="view-split">
        <div className="dna-projects-pane">
          <div className="toolbar" style={{ marginBottom: 10 }}>
            <div className="toolbar-left">
              <div className="search-wrap">
                <Search size={14} />
                <input
                  type="text"
                  className="input search-input"
                  placeholder="Filter projects or stacks..."
                  value={query}
                  onChange={(e) => setQuery(e.target.value)}
                  aria-label="Filter projects"
                />
              </div>
            </div>
          </div>

          <div className="pill-group" role="tablist" aria-label="DNA status filter" style={{ marginBottom: 10 }}>
            {(
              [
                ["all", `All (${projects.length})`],
                ["MISSING", `Missing (${counts.MISSING})`],
                ["OUTDATED", `Needs Update (${counts.OUTDATED})`],
                ["READY", `Ready (${counts.READY})`],
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

          {visible.length === 0 ? (
            <div className="empty-state">No repositories match.</div>
          ) : (
            <div className="dna-project-list">
              {visible.map((p) => {
                const busy = busyRepos.has(p.repo_path);
                const isSelected = selectedPath === p.repo_path;
                const actionLabel =
                  p.status === "MISSING"
                    ? "Create tools"
                    : p.status === "READY"
                    ? "Sync tools"
                    : "Update tools";
                return (
                  <div
                    key={p.repo_path}
                    className={`dna-project-row ${isSelected ? "selected" : ""}`}
                    onClick={() => setSelectedPath(p.repo_path)}
                  >
                    <div style={{ minWidth: 0 }}>
                      <div style={{ display: "flex", alignItems: "center", gap: 8, flexWrap: "wrap" }}>
                        <span className="repo-name">{p.repo}</span>
                        <HoverTip text={DNA_STATUS_HELP[p.status] ?? p.status}>
                          <span className={`status-badge ${statusTone(p.status)}`}>{p.status}</span>
                        </HoverTip>
                        <span className="mini-tag">
                          {p.existing_count}/{p.expected_count} tools
                        </span>
                        {p.default_port && <span className="mini-tag">:{p.default_port}</span>}
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
                      <button
                        type="button"
                        className={`btn btn-sm ${p.status !== "READY" ? "btn-primary" : ""}`}
                        onClick={() => void handleRunCreate(p.repo_path)}
                        disabled={busy || batchBusy}
                        title="Analyze stack and write tailored gloriaTools/*.bat scripts"
                      >
                        <Wrench size={12} className={busy ? "spin" : ""} />
                        <span>{busy ? "Creating..." : actionLabel}</span>
                      </button>
                      <button
                        type="button"
                        className="btn btn-sm"
                        onClick={() => setSelectedPath(p.repo_path)}
                        title="Inspect detected stack and preview scripts"
                      >
                        <Eye size={12} />
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
        </div>

        <div className="editor-pane dna-inspect-pane">
          {!inspectData ? (
            <div className="empty-state">
              {inspectLoading ? "Analyzing repository..." : "Select a project to inspect its DNA."}
            </div>
          ) : (
            <>
              <div className="editor-header">
                <div style={{ minWidth: 0 }}>
                  <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
                    <span style={{ fontFamily: "var(--font-mono)", fontWeight: 700 }}>
                      {inspectData.summary.repo}
                    </span>
                    <span className={`status-badge ${statusTone(inspectData.summary.status)}`}>
                      {inspectData.summary.status}
                    </span>
                  </div>
                  <div className="cli-path selectable">
                    Stacks: {inspectData.summary.stacks.join(", ")}
                    {inspectData.summary.default_port ? ` · Port ${inspectData.summary.default_port}` : ""}
                    {inspectData.summary.main_exe ? ` · ${inspectData.summary.main_exe}` : ""}
                  </div>
                </div>
                <button
                  type="button"
                  className="btn btn-sm btn-primary"
                  disabled={checkedTools.size === 0 || busyRepos.has(inspectData.summary.repo_path)}
                  onClick={() =>
                    void handleRunCreate(inspectData.summary.repo_path, [...checkedTools])
                  }
                >
                  <Wrench size={12} />
                  <span>Create selected ({checkedTools.size})</span>
                </button>
              </div>

              <div className="dna-tool-tabs">
                {inspectData.tools.map((t) => {
                  const isActive = activeTool?.file_name === t.file_name;
                  const isCustom = t.status === "CUSTOM";
                  const checked = checkedTools.has(t.file_name);
                  return (
                    <div
                      key={t.file_name}
                      className={`dna-tool-chip ${isActive ? "active" : ""}`}
                      onClick={() => setActiveToolName(t.file_name)}
                    >
                      {!isCustom && (
                        <button
                          type="button"
                          className="dna-check-btn"
                          onClick={(e) => {
                            e.stopPropagation();
                            toggleToolCheck(t.file_name);
                          }}
                          aria-label={`Toggle ${t.file_name}`}
                        >
                          {checked ? <CheckSquare size={13} /> : <Square size={13} />}
                        </button>
                      )}
                      <span style={{ fontFamily: "var(--font-mono)", fontSize: 12 }}>
                        {t.file_name}
                      </span>
                      <HoverTip text={TOOL_STATUS_HELP[t.status] ?? t.status}>
                        <span className={`status-badge ${toolTone(t.status)}`} style={{ fontSize: 9.5, padding: "1px 5px" }}>
                          {t.status}
                        </span>
                      </HoverTip>
                    </div>
                  );
                })}
              </div>

              {activeTool && (
                <>
                  <div className="dna-tool-desc">
                    <span>{activeTool.description}</span>
                    {activeTool.status === "CUSTOM" && (
                      <span className="mini-tag">Preserved (never overwritten)</span>
                    )}
                  </div>
                  <div className="dna-code-preview selectable">
                    <pre>{activeTool.content}</pre>
                  </div>
                </>
              )}
            </>
          )}
        </div>
      </div>
    </div>
  );
}
