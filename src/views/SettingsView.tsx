import { open } from "@tauri-apps/plugin-dialog";
import { AlertTriangle, FolderOpen, FolderPlus, Save, Trash2 } from "lucide-react";
import { useEffect, useState } from "react";
import type { AppConfig, AppStateSnapshot } from "../types";

interface SettingsViewProps {
  appState: AppStateSnapshot;
  onSaveConfig: (cfg: AppConfig) => Promise<void>;
  onRemoveLegacy: () => Promise<void>;
  onOpenFolder: (path: string) => void;
}

export function SettingsView({
  appState,
  onSaveConfig,
  onRemoveLegacy,
  onOpenFolder,
}: SettingsViewProps) {
  const [brainRepoPath, setBrainRepoPath] = useState(
    appState.config.brain_repo_path ?? ""
  );
  const [scanRoots, setScanRoots] = useState<string[]>(appState.config.scan_roots);
  const [customRepos, setCustomRepos] = useState<string[]>(appState.config.custom_repos);
  const [saving, setSaving] = useState(false);

  useEffect(() => {
    setBrainRepoPath(appState.config.brain_repo_path ?? "");
    setScanRoots(appState.config.scan_roots);
    setCustomRepos(appState.config.custom_repos);
  }, [appState.config]);

  const isDirty =
    (brainRepoPath.trim() || null) !== (appState.config.brain_repo_path ?? null) ||
    JSON.stringify(scanRoots) !== JSON.stringify(appState.config.scan_roots) ||
    JSON.stringify(customRepos) !== JSON.stringify(appState.config.custom_repos);

  const pickDirectory = async (title: string): Promise<string | null> => {
    const selected = await open({ directory: true, multiple: false, title });
    if (typeof selected === "string" && selected.trim()) {
      return selected;
    }
    return null;
  };

  const handlePickBrainRepo = async () => {
    const dir = await pickDirectory("Select Brain Repository Root");
    if (dir) setBrainRepoPath(dir);
  };

  const handleAddScanRoot = async () => {
    const dir = await pickDirectory("Add Workspace Scan Root");
    if (dir && !scanRoots.some((r) => r.toLowerCase() === dir.toLowerCase())) {
      setScanRoots((prev) => [...prev, dir]);
    }
  };

  const handleAddCustomRepo = async () => {
    const dir = await pickDirectory("Add Custom Repository Folder");
    if (dir && !customRepos.some((r) => r.toLowerCase() === dir.toLowerCase())) {
      setCustomRepos((prev) => [...prev, dir]);
    }
  };

  const handleSave = async () => {
    setSaving(true);
    try {
      await onSaveConfig({
        brain_repo_path: brainRepoPath.trim() ? brainRepoPath.trim() : null,
        scan_roots: scanRoots,
        custom_repos: customRepos,
      });
    } finally {
      setSaving(false);
    }
  };

  return (
    <div className="view-scroll">
      <div className="page-header">
        <div>
          <div className="page-eyebrow">CORE SYSTEM // PATH TOPOLOGY</div>
          <h1 className="page-title">Settings &amp; Paths</h1>
          <p className="page-subtitle">
            Configure the Brain source repository, workspace scan roots, and tool targets.
          </p>
        </div>

        <button
          type="button"
          className="btn btn-primary"
          onClick={handleSave}
          disabled={!isDirty || saving}
        >
          <Save size={14} />
          <span>{saving ? "Saving..." : "Save Settings"}</span>
        </button>
      </div>

      <div className="settings-stack">
        <section className="settings-card">
          <h2 className="settings-card-title">Brain Source Repository</h2>
          <p className="settings-card-desc">
            Folder containing <code>brain/init.md</code>, <code>install-brain.bat</code>,
            and <code>skills/</code>.
          </p>
          <div className="path-row">
            <input
              type="text"
              className="input selectable"
              value={brainRepoPath}
              onChange={(e) => setBrainRepoPath(e.target.value)}
              placeholder="e.g. C:\Users\you\Desktop\projectGloria\Brain"
            />
            <button type="button" className="btn" onClick={handlePickBrainRepo}>
              <FolderOpen size={14} />
              <span>Browse</span>
            </button>
          </div>
        </section>

        <section className="settings-card">
          <div
            style={{
              display: "flex",
              alignItems: "center",
              justifyContent: "space-between",
              gap: 10,
            }}
          >
            <div>
              <h2 className="settings-card-title">Scan Root Directories</h2>
              <p className="settings-card-desc">
                All immediate subdirectories inside these roots are scanned in Dashboard.
              </p>
            </div>
            <button type="button" className="btn btn-sm" onClick={handleAddScanRoot}>
              <FolderPlus size={13} />
              <span>Add Root</span>
            </button>
          </div>

          {scanRoots.length === 0 ? (
            <div style={{ fontSize: 12, color: "var(--text-muted)" }}>
              No scan roots configured.
            </div>
          ) : (
            <div className="path-chip-list">
              {scanRoots.map((root) => (
                <div key={root} className="path-item">
                  <span className="selectable">{root}</span>
                  <button
                    type="button"
                    className="btn btn-sm btn-danger"
                    onClick={() =>
                      setScanRoots((prev) => prev.filter((item) => item !== root))
                    }
                    title="Remove scan root"
                  >
                    <Trash2 size={12} />
                  </button>
                </div>
              ))}
            </div>
          )}
        </section>

        <section className="settings-card">
          <div
            style={{
              display: "flex",
              alignItems: "center",
              justifyContent: "space-between",
              gap: 10,
            }}
          >
            <div>
              <h2 className="settings-card-title">Custom Repositories</h2>
              <p className="settings-card-desc">
                Individual repositories outside your scan roots to include in Dashboard.
              </p>
            </div>
            <button type="button" className="btn btn-sm" onClick={handleAddCustomRepo}>
              <FolderPlus size={13} />
              <span>Add Repo</span>
            </button>
          </div>

          {customRepos.length === 0 ? (
            <div style={{ fontSize: 12, color: "var(--text-muted)" }}>
              No custom repositories added.
            </div>
          ) : (
            <div className="path-chip-list">
              {customRepos.map((repo) => (
                <div key={repo} className="path-item">
                  <span className="selectable">{repo}</span>
                  <button
                    type="button"
                    className="btn btn-sm btn-danger"
                    onClick={() =>
                      setCustomRepos((prev) => prev.filter((item) => item !== repo))
                    }
                    title="Remove custom repo"
                  >
                    <Trash2 size={12} />
                  </button>
                </div>
              ))}
            </div>
          )}
        </section>

        <section className="settings-card">
          <h2 className="settings-card-title">Master Brain &amp; Tool Skill Targets</h2>
          <p className="settings-card-desc">
            Read-only locations managed by Compile &amp; Install.
          </p>

          <div className="path-chip-list">
            <div className="path-item">
              <div>
                <strong style={{ color: "var(--text-primary)", marginRight: 8 }}>
                  Master Brain
                </strong>
                <span className="selectable">{appState.master_dir}</span>
              </div>
              <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
                <span
                  className={`status-badge ${appState.master_exists ? "ok" : "muted"}`}
                >
                  {appState.master_exists ? "PRESENT" : "MISSING"}
                </span>
                {appState.master_exists && (
                  <button
                    type="button"
                    className="btn btn-sm"
                    onClick={() => onOpenFolder(appState.master_dir)}
                  >
                    <FolderOpen size={12} />
                  </button>
                )}
              </div>
            </div>

            {appState.tool_paths.map((tp) => (
              <div key={tp.tool} className="path-item">
                <div>
                  <strong style={{ color: "var(--text-primary)", marginRight: 8 }}>
                    {tp.label}
                  </strong>
                  <span className="selectable">{tp.path}</span>
                </div>
                <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
                  <span className={`status-badge ${tp.exists ? "ok" : "muted"}`}>
                    {tp.exists ? "READY" : "NOT CREATED"}
                  </span>
                  {tp.exists && (
                    <button
                      type="button"
                      className="btn btn-sm"
                      onClick={() => onOpenFolder(tp.path)}
                    >
                      <FolderOpen size={12} />
                    </button>
                  )}
                </div>
              </div>
            ))}
          </div>
        </section>

        <section className="settings-card">
          <div
            style={{
              display: "flex",
              alignItems: "center",
              justifyContent: "space-between",
              gap: 12,
            }}
          >
            <div>
              <h2 className="settings-card-title">Legacy Antigravity Cleanup</h2>
              <p className="settings-card-desc selectable">
                Checks for legacy <code>{appState.legacy_skill_path}</code>.
              </p>
            </div>
            {appState.legacy_skill_present ? (
              <button
                type="button"
                className="btn btn-danger"
                onClick={onRemoveLegacy}
              >
                <AlertTriangle size={14} />
                <span>Remove Legacy Skill</span>
              </button>
            ) : (
              <span className="status-badge ok">CLEAN</span>
            )}
          </div>
        </section>
      </div>
    </div>
  );
}
