import { useCallback, useEffect, useMemo, useState } from "react";
import {
  Download,
  FileCode2,
  FolderOpen,
  Hammer,
  RefreshCw,
  Save,
  TerminalSquare,
  X,
} from "lucide-react";
import { api } from "../api";
import { confirmDiscard, sameText, useUnsavedGuard } from "../hooks/useUnsavedGuard";
import type {
  CliActionResult,
  CliConfigDocument,
  CliInfo,
  CliLatest,
  SkillEntry,
  ToastItem,
} from "../types";

interface CliManagerViewProps {
  clis: CliInfo[];
  latest: Record<string, CliLatest>;
  loading: boolean;
  checkingLatest: boolean;
  skills: SkillEntry[];
  installing: boolean;
  /** Owned by App so a running install/update survives switching tabs. */
  busy: Record<string, CliJobKind | undefined>;
  logs: Record<string, CliActionResult>;
  onRunAction: (cli: CliInfo, kind: CliJobKind) => void;
  onRefresh: (forceLatest: boolean) => void;
  onInstallAll: () => void;
  onOpenFolder: (path: string) => void;
  onToast: (tone: ToastItem["tone"], title: string, detail?: string) => void;
}

/** Numeric semver-ish compare: returns >0 when a is newer than b. */
export function compareVersions(a: string, b: string): number {
  const parse = (v: string) => {
    const [core, pre] = v.trim().replace(/^v/i, "").split("-", 2);
    return { nums: core.split(".").map((n) => parseInt(n, 10) || 0), pre: pre ?? "" };
  };
  const pa = parse(a);
  const pb = parse(b);
  for (let i = 0; i < Math.max(pa.nums.length, pb.nums.length); i++) {
    const d = (pa.nums[i] ?? 0) - (pb.nums[i] ?? 0);
    if (d !== 0) return d;
  }
  if (pa.pre === pb.pre) return 0;
  if (!pa.pre) return 1;
  if (!pb.pre) return -1;
  return pa.pre.localeCompare(pb.pre, undefined, { numeric: true });
}

export type CliJobKind = "install" | "update";

export type CliState = "INSTALLED" | "UPDATE AVAILABLE" | "NOT INSTALLED";

export function cliState(cli: CliInfo, latest?: CliLatest): CliState {
  if (!cli.installed) return "NOT INSTALLED";
  if (cli.version && latest?.latest && compareVersions(latest.latest, cli.version) > 0) {
    return "UPDATE AVAILABLE";
  }
  return "INSTALLED";
}

const SOURCE_LABEL: Record<string, string> = {
  native: "native installer",
  npm: "npm global",
  winget: "winget",
  unknown: "unknown",
};

interface EditorTarget {
  id: string;
  key: string;
}

export function CliManagerView({
  clis,
  latest,
  loading,
  checkingLatest,
  skills,
  installing,
  busy,
  logs,
  onRunAction,
  onRefresh,
  onInstallAll,
  onOpenFolder,
  onToast,
}: CliManagerViewProps) {
  const [armed, setArmed] = useState<string | null>(null);
  // Explicit show/hide per CLI; otherwise a failed job's log starts open.
  const [logOpen, setLogOpen] = useState<Record<string, boolean>>({});

  const [target, setTarget] = useState<EditorTarget | null>(null);
  const [doc, setDoc] = useState<CliConfigDocument | null>(null);
  const [draft, setDraft] = useState("");
  const [docLoading, setDocLoading] = useState(false);
  const [saving, setSaving] = useState(false);
  const [saveError, setSaveError] = useState<string | null>(null);

  const dirty = doc ? !sameText(draft, doc.content) : false;
  useUnsavedGuard("cli-config", dirty, doc ? doc.path : "CLI config");

  // Two-click confirm resets after a few seconds.
  useEffect(() => {
    if (!armed) return;
    const t = window.setTimeout(() => setArmed(null), 4000);
    return () => window.clearTimeout(t);
  }, [armed]);

  const runAction = (cli: CliInfo, kind: CliJobKind) => {
    const armKey = `${kind}:${cli.id}`;
    if (armed !== armKey) {
      setArmed(armKey);
      return;
    }
    setArmed(null);
    setLogOpen((o) => {
      const next = { ...o };
      delete next[cli.id];
      return next;
    });
    onRunAction(cli, kind);
  };

  const loadDoc = useCallback(
    async (t: EditorTarget) => {
      setDocLoading(true);
      setSaveError(null);
      try {
        const d = await api.readCliConfig(t.id, t.key);
        setTarget(t);
        setDoc(d);
        setDraft(d.content);
      } catch (err) {
        onToast("error", "Could not open config", String(err));
      } finally {
        setDocLoading(false);
      }
    },
    [onToast]
  );

  const openEditor = (t: EditorTarget) => {
    if (target && target.id === t.id && target.key === t.key) return;
    if (!confirmDiscard("cli-config")) return;
    void loadDoc(t);
  };

  const closeEditor = () => {
    if (!confirmDiscard("cli-config")) return;
    setTarget(null);
    setDoc(null);
    setDraft("");
    setSaveError(null);
  };

  const handleSave = async () => {
    if (!target) return;
    setSaving(true);
    setSaveError(null);
    try {
      const saved = await api.saveCliConfig(target.id, target.key, draft);
      setDoc(saved);
      setDraft(saved.content);
      onToast("success", "Config saved", `${saved.path} (previous copy kept as .bak)`);
      onRefresh(false);
    } catch (err) {
      setSaveError(String(err));
    } finally {
      setSaving(false);
    }
  };

  const skillsByTool = useMemo(() => {
    const map: Record<string, SkillEntry[]> = {};
    for (const s of skills) (map[s.tool] ??= []).push(s);
    return map;
  }, [skills]);

  const skillTone = (status: string) =>
    status === "SYNCED" ? "ok" : status === "OUT_OF_SYNC" ? "drift" : "muted";

  const stateTone = (s: CliState) =>
    s === "INSTALLED" ? "ok" : s === "UPDATE AVAILABLE" ? "drift" : "muted";

  return (
    <div className="view-scroll">
      <div className="page-header">
        <div>
          <div className="page-eyebrow">AGENT RUNTIMES // CLI CONTROL</div>
          <h1 className="page-title">Coding-Agent CLIs</h1>
          <p className="page-subtitle">
            Detect, install, update and configure Claude Code, Codex and Antigravity CLI.
            Installs and updates go through winget (or npm / the native updater when that is
            how the CLI was installed).
          </p>
        </div>
        <div style={{ display: "flex", gap: 8 }}>
          <button
            type="button"
            className="btn"
            onClick={() => onRefresh(true)}
            disabled={loading || checkingLatest}
          >
            <RefreshCw size={14} className={loading || checkingLatest ? "spin" : ""} />
            <span>{checkingLatest ? "Checking updates..." : "Recheck"}</span>
          </button>
          <button
            type="button"
            className="btn btn-primary"
            onClick={onInstallAll}
            disabled={installing}
          >
            <Hammer size={14} className={installing ? "spin" : ""} />
            <span>{installing ? "Deploying..." : "Compile & Deploy Skills"}</span>
          </button>
        </div>
      </div>

      {loading && clis.length === 0 ? (
        <div className="empty-state">Detecting installed CLIs...</div>
      ) : (
        <div className="cli-grid">
          {clis.map((cli) => {
            const lt = latest[cli.id];
            const state = cliState(cli, lt);
            const action = busy[cli.id];
            const log = logs[cli.id];
            const showLog = !!log && (logOpen[cli.id] ?? !log.ok);
            const toolSkills = skillsByTool[cli.skill_tool] ?? [];
            const installArmed = armed === `install:${cli.id}`;
            const updateArmed = armed === `update:${cli.id}`;

            return (
              <section key={cli.id} className="settings-card cli-card" aria-label={cli.name}>
                <div className="cli-card-head">
                  <div style={{ display: "flex", alignItems: "center", gap: 8, minWidth: 0 }}>
                    <TerminalSquare size={16} />
                    <h2 className="settings-card-title">{cli.name}</h2>
                  </div>
                  <span className={`status-badge ${stateTone(state)}`}>{state}</span>
                </div>

                <dl className="cli-kv">
                  <dt>Installed</dt>
                  <dd>
                    <code>{cli.version ?? (cli.installed ? "unknown" : "—")}</code>
                  </dd>
                  <dt>Latest</dt>
                  <dd>
                    <code>
                      {lt?.latest ?? (checkingLatest ? "checking..." : lt?.error ? "unavailable" : "—")}
                    </code>
                    {lt?.source && <span className="cli-muted"> via {lt.source}</span>}
                  </dd>
                  <dt>Source</dt>
                  <dd>{cli.installed ? SOURCE_LABEL[cli.source] ?? cli.source : "—"}</dd>
                  <dt>Executable</dt>
                  <dd className="selectable cli-path" title={cli.exe_path ?? ""}>
                    {cli.exe_path ?? "not found on PATH"}
                  </dd>
                  <dt>Package</dt>
                  <dd>
                    <code>{cli.winget_id}</code>
                    {cli.npm_package && <span className="cli-muted"> · npm {cli.npm_package}</span>}
                  </dd>
                  <dt>Docs</dt>
                  <dd className="selectable cli-path">{cli.docs_url}</dd>
                </dl>

                {cli.error && <div className="alert-strip warn">{cli.error}</div>}

                <div className="cli-actions">
                  {!cli.installed ? (
                    <button
                      type="button"
                      className={`btn btn-sm ${installArmed ? "btn-danger" : "btn-primary"}`}
                      onClick={() => runAction(cli, "install")}
                      disabled={!!action}
                      title={`winget install --id ${cli.winget_id}`}
                    >
                      <Download size={12} className={action === "install" ? "spin" : ""} />
                      <span>
                        {action === "install"
                          ? "Installing..."
                          : installArmed
                          ? "Click again to install"
                          : "Install"}
                      </span>
                    </button>
                  ) : (
                    <button
                      type="button"
                      className={`btn btn-sm ${updateArmed ? "btn-danger" : state === "UPDATE AVAILABLE" ? "btn-primary" : ""}`}
                      onClick={() => runAction(cli, "update")}
                      disabled={!!action}
                    >
                      <RefreshCw size={12} className={action === "update" ? "spin" : ""} />
                      <span>
                        {action === "update"
                          ? "Updating..."
                          : updateArmed
                          ? "Click again to update"
                          : "Update"}
                      </span>
                    </button>
                  )}
                  <button
                    type="button"
                    className="btn btn-sm"
                    onClick={() => onOpenFolder(cli.config_dir)}
                    disabled={!cli.config_dir_exists}
                    title={cli.config_dir}
                  >
                    <FolderOpen size={12} />
                    <span>Config folder</span>
                  </button>
                  <button
                    type="button"
                    className="btn btn-sm"
                    onClick={() => onOpenFolder(cli.skills_dir)}
                    disabled={!cli.skills_dir_exists}
                    title={cli.skills_dir}
                  >
                    <FolderOpen size={12} />
                    <span>Skills folder</span>
                  </button>
                  {log && (
                    <button
                      type="button"
                      className="btn btn-sm"
                      onClick={() => setLogOpen((o) => ({ ...o, [cli.id]: !showLog }))}
                      aria-expanded={showLog}
                    >
                      <span>{showLog ? "Hide log" : "Show log"}</span>
                    </button>
                  )}
                </div>

                {showLog && (
                  <pre className="cli-log selectable">
                    {`> ${log.command}\n${log.log || "(no output)"}\n[exit ${log.exit_code ?? "?"}]`}
                  </pre>
                )}

                <div>
                  <div className="cli-section-label">Config files</div>
                  <div className="cli-chip-row">
                    {cli.config_files.map((f) => {
                      const active = target?.id === cli.id && target.key === f.key;
                      return (
                        <button
                          key={f.key}
                          type="button"
                          className={`filter-pill ${active ? "active" : ""}`}
                          onClick={() => openEditor({ id: cli.id, key: f.key })}
                          title={f.path}
                          aria-pressed={active}
                        >
                          <FileCode2 size={11} style={{ verticalAlign: "middle", marginRight: 4 }} />
                          {f.label}
                          {!f.exists && <span className="cli-muted"> (new)</span>}
                        </button>
                      );
                    })}
                  </div>
                </div>

                <div>
                  <div className="cli-section-label">Brain skills ({toolSkills.length})</div>
                  {toolSkills.length === 0 ? (
                    <div className="cli-muted">No Brain skills for this tool.</div>
                  ) : (
                    <ul className="cli-skill-list">
                      {toolSkills.map((s) => (
                        <li key={s.skill_name}>
                          <span className="selectable">{s.skill_name}</span>
                          <span
                            className={`status-badge ${skillTone(s.status)}`}
                            style={{ fontSize: 9.5, padding: "1px 6px" }}
                          >
                            {s.status.replace(/_/g, " ")}
                          </span>
                        </li>
                      ))}
                    </ul>
                  )}
                </div>
              </section>
            );
          })}
        </div>
      )}

      {target && (
        <section className="settings-card cli-editor" aria-label="Config editor">
          <div className="cli-card-head">
            <div style={{ minWidth: 0 }}>
              <h2 className="settings-card-title">
                {clis.find((c) => c.id === target.id)?.name} · {target.key}
                {dirty && (
                  <span className="mini-tag accent" style={{ marginLeft: 8 }}>
                    UNSAVED
                  </span>
                )}
              </h2>
              <div className="cli-path selectable">{doc?.path}</div>
            </div>
            <div style={{ display: "flex", gap: 8 }}>
              <button
                type="button"
                className="btn btn-sm btn-primary"
                onClick={handleSave}
                disabled={!dirty || saving || docLoading}
              >
                <Save size={12} />
                <span>{saving ? "Saving..." : doc?.exists ? "Save" : "Create file"}</span>
              </button>
              <button type="button" className="btn btn-sm" onClick={closeEditor} aria-label="Close editor">
                <X size={12} />
              </button>
            </div>
          </div>
          {saveError && <div className="alert-strip warn selectable">{saveError}</div>}
          <div className="code-editor-wrap" style={{ minHeight: 280 }}>
            <textarea
              className="code-textarea"
              value={draft}
              onChange={(e) => setDraft(e.target.value)}
              spellCheck={false}
              disabled={docLoading}
              aria-label={`${target.key} contents`}
            />
          </div>
          <div className="cli-muted">
            {doc?.format.toUpperCase()} is validated before saving; the previous file is kept as{" "}
            <code>.bak</code>.
          </div>
        </section>
      )}
    </div>
  );
}
