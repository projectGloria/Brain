import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { api } from "./api";
import { CyberspaceBg } from "./components/CyberspaceBg";
import { Sidebar } from "./components/Sidebar";
import { SplashScreen } from "./components/SplashScreen";
import { TitleBar } from "./components/TitleBar";
import { ToastHost } from "./components/ToastHost";
import { useAgentRuns } from "./hooks/useAgentRuns";
import { confirmDiscard } from "./hooks/useUnsavedGuard";
import type {
  AgentRunPreset,
  AppConfig,
  AppStateSnapshot,
  CliActionResult,
  CliInfo,
  CliLatest,
  GitignoreApplyResult,
  GitignoreTemplate,
  NavTab,
  RepoStatus,
  RuleSummary,
  SkillEntry,
  ToastItem,
} from "./types";
import { AgentRunsView } from "./views/AgentRunsView";
import { CliManagerView, cliState, type CliJobKind } from "./views/CliManagerView";
import { DashboardView } from "./views/DashboardView";
import { GitignoreView } from "./views/GitignoreView";
import { RuleEditorView } from "./views/RuleEditorView";
import { SettingsView } from "./views/SettingsView";
import { SkillManagerView } from "./views/SkillManagerView";

const plural = (n: number, one: string, many: string) => `${n} ${n === 1 ? one : many}`;

export function App() {
  const [showSplash, setShowSplash] = useState(true);
  const [activeTab, setActiveTab] = useState<NavTab>("dashboard");
  const [appState, setAppState] = useState<AppStateSnapshot | null>(null);
  const [stateError, setStateError] = useState<string | null>(null);
  const [repos, setRepos] = useState<RepoStatus[]>([]);
  const [rules, setRules] = useState<RuleSummary[]>([]);
  const [skills, setSkills] = useState<SkillEntry[]>([]);
  const [selectedRuleName, setSelectedRuleName] = useState<string | null>(null);

  const [clis, setClis] = useState<CliInfo[]>([]);
  const [cliLatest, setCliLatest] = useState<Record<string, CliLatest>>({});
  const [loadingClis, setLoadingClis] = useState(false);
  const [checkingLatest, setCheckingLatest] = useState(false);
  const [cliBusy, setCliBusy] = useState<Record<string, CliJobKind | undefined>>({});
  const [cliLogs, setCliLogs] = useState<Record<string, CliActionResult>>({});

  const [gitignoreTemplate, setGitignoreTemplate] = useState<GitignoreTemplate | null>(null);
  const [gitignoreError, setGitignoreError] = useState<string | null>(null);

  const [scanningRepos, setScanningRepos] = useState(false);
  const [syncingRepos, setSyncingRepos] = useState<Set<string>>(new Set());
  const [syncingAll, setSyncingAll] = useState(false);
  const [installing, setInstalling] = useState(false);

  const [toasts, setToasts] = useState<ToastItem[]>([]);
  const toastIdRef = useRef(1);

  const pushToast = useCallback(
    (tone: ToastItem["tone"], title: string, detail?: string) => {
      const id = toastIdRef.current++;
      setToasts((prev) => [...prev.slice(-3), { id, tone, title, detail }]);
      // Errors stay until dismissed so they can be read and copied.
      if (tone !== "error") {
        window.setTimeout(() => {
          setToasts((prev) => prev.filter((t) => t.id !== id));
        }, 4200);
      }
    },
    []
  );

  const agentRuns = useAgentRuns(pushToast);
  const [runPreset, setRunPreset] = useState<AgentRunPreset | null>(null);

  const dismissToast = useCallback((id: number) => {
    setToasts((prev) => prev.filter((t) => t.id !== id));
  }, []);

  const refreshState = useCallback(async () => {
    try {
      const snap = await api.getAppState();
      setAppState(snap);
      setStateError(null);
      return snap;
    } catch (err) {
      setStateError(String(err));
      return null;
    }
  }, []);

  const refreshRepos = useCallback(async () => {
    setScanningRepos(true);
    try {
      const list = await api.scanRepos();
      setRepos(list);
    } catch (err) {
      pushToast("error", "Repository scan failed", String(err));
    } finally {
      setScanningRepos(false);
    }
  }, [pushToast]);

  const refreshRules = useCallback(async () => {
    try {
      setRules(await api.listRules());
    } catch {
      setRules([]);
    }
  }, []);

  const refreshSkills = useCallback(async () => {
    try {
      setSkills(await api.listSkills());
    } catch {
      setSkills([]);
    }
  }, []);

  const refreshGitignore = useCallback(async () => {
    try {
      setGitignoreTemplate(await api.readGitignoreTemplate());
      setGitignoreError(null);
    } catch (err) {
      setGitignoreTemplate(null);
      setGitignoreError(String(err));
    }
  }, []);

  const refreshLatest = useCallback(async (force: boolean) => {
    setCheckingLatest(true);
    try {
      const list = await api.cliLatestVersions(force);
      setCliLatest(Object.fromEntries(list.map((l) => [l.id, l])));
    } catch {
      // Latest versions are optional; the cards just show "unknown".
    } finally {
      setCheckingLatest(false);
    }
  }, []);

  const refreshClis = useCallback(
    async (forceLatest = false) => {
      setLoadingClis(true);
      try {
        setClis(await api.listClis());
      } catch (err) {
        pushToast("error", "Could not detect CLIs", String(err));
      } finally {
        setLoadingClis(false);
      }
      // winget is slow, so the latest versions fill in after the list shows.
      void refreshLatest(forceLatest);
    },
    [pushToast, refreshLatest]
  );

  const refreshAll = useCallback(async () => {
    await refreshState();
    await Promise.all([
      refreshRepos(),
      refreshRules(),
      refreshSkills(),
      refreshGitignore(),
      refreshClis(false),
    ]);
  }, [refreshState, refreshRepos, refreshRules, refreshSkills, refreshGitignore, refreshClis]);

  useEffect(() => {
    void refreshAll();
  }, [refreshAll]);

  const selectTab = (tab: NavTab) => {
    if (tab === activeTab) return;
    if (!confirmDiscard()) return;
    setActiveTab(tab);
  };

  const handleInstallAll = async () => {
    setInstalling(true);
    try {
      const res = await api.installBrain();
      await Promise.all([refreshState(), refreshRepos(), refreshSkills()]);
      const stubNote =
        res.stub_files.length > 0 ? ` (${res.stub_files.length} file(s) contain stubs)` : "";
      pushToast(
        res.stub_files.length > 0 ? "warning" : "success",
        `Compiled & Installed (${res.fingerprint})`,
        `Master: ${res.master_files_count} files, Skills installed: ${res.installed_skills_count}${stubNote}`
      );
    } catch (err) {
      pushToast("error", "Compile & Install failed", String(err));
    } finally {
      setInstalling(false);
    }
  };

  const handleCopyFingerprint = async () => {
    if (!appState) return;
    const marker = `<!-- brain-sync: ${appState.master_fingerprint} -->`;
    try {
      await navigator.clipboard.writeText(marker);
      pushToast("info", "Copied sync marker", marker);
    } catch {
      pushToast("warning", "Clipboard unavailable", marker);
    }
  };

  const replaceRepo = (updated: RepoStatus) =>
    setRepos((prev) => prev.map((r) => (r.repo_path === updated.repo_path ? updated : r)));

  const handleRecheckRepo = async (repoPath: string) => {
    try {
      replaceRepo(await api.checkRepo(repoPath));
    } catch (err) {
      pushToast("error", "Recheck failed", String(err));
    }
  };

  const handleSyncRepo = async (repoPath: string) => {
    setSyncingRepos((s) => new Set(s).add(repoPath));
    try {
      const res = await api.syncRepo(repoPath);
      const prunedMsg = res.pruned.length > 0 ? `, pruned ${res.pruned.length}` : "";
      let name = repoPath;
      try {
        const updated = await api.checkRepo(repoPath);
        replaceRepo(updated);
        name = updated.repo;
      } catch (err) {
        pushToast("warning", "Synced, but the recheck failed", String(err));
      }
      pushToast(
        "success",
        `Synced .brain/ in ${name}`,
        `${res.copied.length} rules copied${prunedMsg}. Marker: ${res.fingerprint}`
      );
    } catch (err) {
      pushToast("error", "Sync failed", String(err));
    } finally {
      setSyncingRepos((s) => {
        const next = new Set(s);
        next.delete(repoPath);
        return next;
      });
    }
  };

  const handleSyncAllDrifted = async (repoPaths: string[]) => {
    if (repoPaths.length === 0) return;
    setSyncingAll(true);
    try {
      const results = await api.syncAllRepos(repoPaths);
      const failed = results.filter((r) => !r.ok);
      const okCount = results.length - failed.length;
      if (failed.length === 0) {
        pushToast(
          "success",
          `Synced ${plural(okCount, "repository", "repositories")}`,
          "Mirrored ~/.brain/*.md into .brain/ and updated .git/info/exclude."
        );
      } else {
        pushToast(
          okCount > 0 ? "warning" : "error",
          `Synced ${okCount}, failed ${failed.length}`,
          failed.map((f) => `${f.repo_path}: ${f.error ?? "unknown error"}`).join("\n")
        );
      }
    } catch (err) {
      pushToast("error", "Batch sync failed", String(err));
    } finally {
      await refreshRepos();
      setSyncingAll(false);
    }
  };

  const handleRemoveLegacy = async () => {
    try {
      const removed = await api.removeLegacySkill();
      await refreshState();
      pushToast(
        removed ? "success" : "info",
        removed ? "Removed legacy skill" : "Legacy skill not present"
      );
    } catch (err) {
      pushToast("error", "Could not remove legacy skill", String(err));
    }
  };

  const handleSaveSkill = async (tool: string, skillName: string, content: string) => {
    try {
      await api.saveSkill(tool, skillName, content);
      await Promise.all([refreshSkills(), refreshState()]);
      pushToast(
        "success",
        `Saved ${tool}/${skillName}/SKILL.md`,
        "Run Compile & Deploy to push to your tool folder."
      );
    } catch (err) {
      pushToast("error", "Failed to save skill", String(err));
    }
  };

  const handleSaveConfig = async (cfg: AppConfig) => {
    try {
      const snap = await api.updateConfig(cfg);
      setAppState(snap);
      setStateError(null);
      pushToast("success", "Settings saved");
    } catch (err) {
      pushToast("error", "Invalid settings", String(err));
      return;
    }
    await Promise.all([refreshRepos(), refreshRules(), refreshSkills(), refreshGitignore()]);
  };

  const handleOpenFolder = (path: string) => {
    api.openInExplorer(path).catch((err) => {
      pushToast("error", "Could not open folder", String(err));
    });
  };

  const handleSaveGitignore = async (content: string) => {
    try {
      setGitignoreTemplate(await api.saveGitignoreTemplate(content));
      pushToast("success", "Saved .gitignore draft", "templates/gitignore in the Brain repo.");
      await refreshRepos();
      return true;
    } catch (err) {
      pushToast("error", "Could not save the .gitignore draft", String(err));
      return false;
    }
  };

  const handleApplyGitignore = async (repoPaths: string[]): Promise<GitignoreApplyResult[]> => {
    if (repoPaths.length === 0) return [];
    let results: GitignoreApplyResult[] = [];
    try {
      results = await api.applyGitignoreMany(repoPaths);
      const failed = results.filter((r) => !r.ok);
      const okCount = results.length - failed.length;
      if (failed.length === 0) {
        pushToast("success", `Wrote .gitignore to ${plural(okCount, "repository", "repositories")}`);
      } else {
        pushToast(
          okCount > 0 ? "warning" : "error",
          `.gitignore: wrote ${okCount}, failed ${failed.length}`,
          failed.map((f) => `${f.repo_path}: ${f.error ?? "unknown error"}`).join("\n")
        );
      }
    } catch (err) {
      pushToast("error", "Could not write .gitignore", String(err));
    }
    // Refresh only the touched rows; a full rescan is slower.
    await Promise.all(
      repoPaths.map((p) =>
        api
          .checkRepo(p)
          .then(replaceRepo)
          .catch(() => {})
      )
    );
    return results;
  };

  const handleCliAction = async (cli: CliInfo, kind: CliJobKind) => {
    if (cliBusy[cli.id]) return;
    setCliBusy((b) => ({ ...b, [cli.id]: kind }));
    try {
      const res = kind === "install" ? await api.installCli(cli.id) : await api.updateCli(cli.id);
      setCliLogs((l) => ({ ...l, [cli.id]: res }));
      setClis((prev) => prev.map((c) => (c.id === res.info.id ? res.info : c)));
      if (res.ok) {
        pushToast(
          "success",
          `${cli.name} ${kind === "install" ? "installed" : "updated"}`,
          res.info.version ? `Now at ${res.info.version}` : res.command
        );
      } else {
        pushToast(
          "error",
          `${cli.name} ${kind} failed (exit ${res.exit_code ?? "?"})`,
          "See the log on the CLI's card."
        );
      }
    } catch (err) {
      pushToast("error", `${cli.name} ${kind} failed`, String(err));
    } finally {
      setCliBusy((b) => ({ ...b, [cli.id]: undefined }));
    }
  };

  const handleRunInit = (repoPath: string) => {
    if (!confirmDiscard()) return;
    setRunPreset({ repo_path: repoPath, skill: "gloria-init-brain", nonce: Date.now() });
    setActiveTab("runs");
  };

  /** Same definition as the Dashboard "Needs Attention" filter and KPI. */
  const attentionCount = useMemo(
    () => repos.filter((r) => r.status !== "UP TO DATE" && r.status !== "NO AGENTS.MD").length,
    [repos]
  );

  const cliAttentionCount = useMemo(
    () => clis.filter((c) => cliState(c, cliLatest[c.id]) !== "INSTALLED").length,
    [clis, cliLatest]
  );

  const gitignoreAttentionCount = useMemo(
    () =>
      repos.filter(
        (r) =>
          r.is_git_repo &&
          r.status !== "ERROR" &&
          (r.gitignore_status === "MISSING" || r.gitignore_status === "DIFFERS")
      ).length,
    [repos]
  );

  const skillsAttentionCount = useMemo(
    () => skills.filter((s) => s.status !== "SYNCED").length,
    [skills]
  );

  const ruleCount = useMemo(() => rules.filter((r) => !r.is_init).length, [rules]);

  return (
    <>
      <CyberspaceBg />

      {showSplash && (
        <SplashScreen
          fingerprint={appState?.master_fingerprint ?? "............"}
          ruleCount={ruleCount}
          repoCount={repos.length}
          onComplete={() => setShowSplash(false)}
        />
      )}

      <div className="app-shell">
        <TitleBar
          fingerprint={appState?.master_fingerprint ?? "............"}
          masterInSync={appState?.master_in_sync_with_repo ?? true}
          installing={installing}
          onInstallAll={handleInstallAll}
          onCopyFingerprint={handleCopyFingerprint}
          onReplaySplash={() => setShowSplash(true)}
        />

        <div className="workspace-body">
          <Sidebar
            activeTab={activeTab}
            onSelectTab={selectTab}
            driftCount={attentionCount}
            ruleCount={ruleCount}
            stubCount={appState?.stub_files.length ?? 0}
            cliAttentionCount={cliAttentionCount}
            cliBusy={Object.values(cliBusy).some(Boolean)}
            runningAgents={agentRuns.runningCount}
            gitignoreAttentionCount={gitignoreAttentionCount}
            skillsAttentionCount={skillsAttentionCount}
            masterExists={appState?.master_exists ?? false}
            masterFingerprint={appState?.master_fingerprint ?? "............"}
            masterInSync={appState?.master_in_sync_with_repo ?? true}
            installing={installing}
            legacySkillPresent={appState?.legacy_skill_present ?? false}
            onRemoveLegacy={handleRemoveLegacy}
          />

          <main className="main-pane">
            {stateError && (
              <div className="alert-strip warn selectable app-error-banner">
                <span>Could not load the Brain state: {stateError}</span>
                <button type="button" className="btn btn-sm" onClick={() => void refreshAll()}>
                  Retry
                </button>
                {activeTab !== "settings" && (
                  <button type="button" className="btn btn-sm" onClick={() => selectTab("settings")}>
                    Open Settings
                  </button>
                )}
              </div>
            )}

            {activeTab === "dashboard" && (
              <DashboardView
                repos={repos}
                loading={scanningRepos}
                syncingRepos={syncingRepos}
                syncingAll={syncingAll}
                gitignoreReady={!!gitignoreTemplate?.exists}
                onRescan={refreshRepos}
                onRecheck={handleRecheckRepo}
                onSyncRepo={handleSyncRepo}
                onSyncAllDrifted={handleSyncAllDrifted}
                onAddGitignore={(p) => handleApplyGitignore([p])}
                onOpenFolder={handleOpenFolder}
                onRunInit={handleRunInit}
                onToast={pushToast}
              />
            )}

            {activeTab === "rules" && (
              <RuleEditorView
                rules={rules}
                selectedRuleName={selectedRuleName}
                onSelectRule={setSelectedRuleName}
                reloadKey={appState?.brain_dir ?? null}
                onRulesChanged={async () => {
                  await Promise.all([refreshRules(), refreshState()]);
                }}
                onInstallAll={handleInstallAll}
                onToast={pushToast}
              />
            )}

            {activeTab === "skills" && (
              <SkillManagerView
                skills={skills}
                installing={installing}
                legacySkillPresent={appState?.legacy_skill_present ?? false}
                onInstallAll={handleInstallAll}
                onRemoveLegacy={handleRemoveLegacy}
                onSaveSkill={handleSaveSkill}
                onJumpToInitRule={() => {
                  if (!confirmDiscard()) return;
                  setSelectedRuleName("init.md");
                  setActiveTab("rules");
                }}
              />
            )}

            {activeTab === "clis" && (
              <CliManagerView
                clis={clis}
                latest={cliLatest}
                loading={loadingClis}
                checkingLatest={checkingLatest}
                skills={skills}
                installing={installing}
                busy={cliBusy}
                logs={cliLogs}
                onRunAction={(cli, kind) => void handleCliAction(cli, kind)}
                onRefresh={(force) => void refreshClis(force)}
                onInstallAll={handleInstallAll}
                onOpenFolder={handleOpenFolder}
                onToast={pushToast}
              />
            )}

            {activeTab === "runs" && (
              <AgentRunsView
                runs={agentRuns}
                clis={clis}
                skills={skills}
                repos={repos}
                preset={runPreset}
                onOpenFolder={handleOpenFolder}
                onToast={pushToast}
              />
            )}

            {activeTab === "gitignore" && (
              <GitignoreView
                template={gitignoreTemplate}
                templateError={gitignoreError}
                repos={repos}
                loadingRepos={scanningRepos}
                onSaveTemplate={handleSaveGitignore}
                onReloadTemplate={() => void refreshGitignore()}
                onApply={handleApplyGitignore}
                onRescan={refreshRepos}
                onOpenFolder={handleOpenFolder}
              />
            )}

            {activeTab === "settings" &&
              (appState ? (
                <SettingsView
                  appState={appState}
                  onSaveConfig={handleSaveConfig}
                  onRemoveLegacy={handleRemoveLegacy}
                  onOpenFolder={handleOpenFolder}
                />
              ) : (
                <div className="view-scroll">
                  <div className="empty-state">
                    {stateError
                      ? "Settings could not load. Fix the config file shown above, or Retry."
                      : "Loading settings..."}
                  </div>
                </div>
              ))}
          </main>
        </div>

        <ToastHost toasts={toasts} onDismiss={dismissToast} />
      </div>
    </>
  );
}

export default App;
