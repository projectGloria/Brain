import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { api } from "./api";
import { CyberspaceBg } from "./components/CyberspaceBg";
import { Sidebar } from "./components/Sidebar";
import { SplashScreen } from "./components/SplashScreen";
import { TitleBar } from "./components/TitleBar";
import { ToastHost } from "./components/ToastHost";
import type {
  AppConfig,
  AppStateSnapshot,
  NavTab,
  RepoStatus,
  RuleSummary,
  SkillEntry,
  ToastItem,
} from "./types";
import { DashboardView } from "./views/DashboardView";
import { RuleEditorView } from "./views/RuleEditorView";
import { SettingsView } from "./views/SettingsView";
import { SkillManagerView } from "./views/SkillManagerView";

export function App() {
  const [showSplash, setShowSplash] = useState(true);
  const [activeTab, setActiveTab] = useState<NavTab>("dashboard");
  const [appState, setAppState] = useState<AppStateSnapshot | null>(null);
  const [repos, setRepos] = useState<RepoStatus[]>([]);
  const [rules, setRules] = useState<RuleSummary[]>([]);
  const [skills, setSkills] = useState<SkillEntry[]>([]);
  const [selectedRuleName, setSelectedRuleName] = useState<string | null>(null);

  const [scanningRepos, setScanningRepos] = useState(false);
  const [syncingRepo, setSyncingRepo] = useState<string | null>(null);
  const [syncingAll, setSyncingAll] = useState(false);
  const [installing, setInstalling] = useState(false);

  const [toasts, setToasts] = useState<ToastItem[]>([]);
  const toastIdRef = useRef(1);

  const pushToast = useCallback(
    (tone: ToastItem["tone"], title: string, detail?: string) => {
      const id = toastIdRef.current++;
      setToasts((prev) => [...prev.slice(-3), { id, tone, title, detail }]);
      window.setTimeout(() => {
        setToasts((prev) => prev.filter((t) => t.id !== id));
      }, 4200);
    },
    []
  );

  const dismissToast = useCallback((id: number) => {
    setToasts((prev) => prev.filter((t) => t.id !== id));
  }, []);

  const refreshState = useCallback(async () => {
    const snap = await api.getAppState();
    setAppState(snap);
    return snap;
  }, []);

  const refreshRepos = useCallback(async () => {
    setScanningRepos(true);
    try {
      const list = await api.scanRepos();
      setRepos(list);
    } finally {
      setScanningRepos(false);
    }
  }, []);

  const refreshRules = useCallback(async () => {
    try {
      const list = await api.listRules();
      setRules(list);
    } catch {
      setRules([]);
    }
  }, []);

  const refreshSkills = useCallback(async () => {
    try {
      const list = await api.listSkills();
      setSkills(list);
    } catch {
      setSkills([]);
    }
  }, []);

  const refreshAll = useCallback(async () => {
    try {
      await refreshState();
      await Promise.all([refreshRepos(), refreshRules(), refreshSkills()]);
    } catch (err) {
      pushToast("error", "Failed to load Brain state", String(err));
    }
  }, [refreshState, refreshRepos, refreshRules, refreshSkills, pushToast]);

  useEffect(() => {
    refreshAll();
  }, [refreshAll]);

  const handleInstallAll = async () => {
    setInstalling(true);
    try {
      const res = await api.installBrain();
      await Promise.all([refreshState(), refreshRepos(), refreshSkills()]);
      const stubNote =
        res.stub_files.length > 0
          ? ` (${res.stub_files.length} file(s) contain stubs)`
          : "";
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

  const handleRecheckRepo = async (repoPath: string) => {
    try {
      const updated = await api.checkRepo(repoPath);
      setRepos((prev) =>
        prev.map((r) => (r.repo_path === repoPath ? updated : r))
      );
    } catch (err) {
      pushToast("error", "Recheck failed", String(err));
    }
  };

  const handleSyncRepo = async (repoPath: string) => {
    setSyncingRepo(repoPath);
    try {
      const res = await api.syncRepo(repoPath);
      const updated = await api.checkRepo(repoPath);
      setRepos((prev) =>
        prev.map((r) => (r.repo_path === repoPath ? updated : r))
      );
      const prunedMsg =
        res.pruned.length > 0 ? `, pruned ${res.pruned.length}` : "";
      pushToast(
        "success",
        `Synced .brain/ in ${updated.repo}`,
        `${res.copied.length} rules copied${prunedMsg}. Marker: ${res.fingerprint}`
      );
    } catch (err) {
      pushToast("error", "Sync failed", String(err));
    } finally {
      setSyncingRepo(null);
    }
  };

  const handleSyncAllDrifted = async (repoPaths: string[]) => {
    if (repoPaths.length === 0) return;
    setSyncingAll(true);
    try {
      const results = await api.syncAllRepos(repoPaths);
      await refreshRepos();
      pushToast(
        "success",
        `Synced ${results.length} repositor${results.length === 1 ? "y" : "ies"}`,
        "Mirrored ~/.brain/*.md into .brain/ and updated .git/info/exclude."
      );
    } catch (err) {
      pushToast("error", "Batch sync failed", String(err));
    } finally {
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

  const handleSaveSkill = async (
    tool: string,
    skillName: string,
    content: string
  ) => {
    try {
      await api.saveSkill(tool, skillName, content);
      await refreshSkills();
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
      await Promise.all([refreshRepos(), refreshRules(), refreshSkills()]);
      pushToast("success", "Settings saved");
    } catch (err) {
      pushToast("error", "Invalid settings", String(err));
    }
  };

  const handleOpenFolder = (path: string) => {
    api.openInExplorer(path).catch((err) => {
      pushToast("error", "Could not open folder", String(err));
    });
  };

  const driftCount = useMemo(
    () => repos.filter((r) => r.status !== "UP TO DATE").length,
    [repos]
  );

  return (
    <>
      <CyberspaceBg />

      {showSplash && (
        <SplashScreen
          fingerprint={appState?.master_fingerprint ?? "............"}
          ruleCount={rules.length}
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
            onSelectTab={setActiveTab}
            driftCount={driftCount}
            ruleCount={rules.length}
            stubCount={appState?.stub_files.length ?? 0}
            masterFingerprint={appState?.master_fingerprint ?? "............"}
            masterInSync={appState?.master_in_sync_with_repo ?? true}
            installing={installing}
            legacySkillPresent={appState?.legacy_skill_present ?? false}
            onRemoveLegacy={handleRemoveLegacy}
          />

          <main className="main-pane">
            {activeTab === "dashboard" && (
              <DashboardView
                repos={repos}
                loading={scanningRepos}
                syncingRepo={syncingRepo}
                syncingAll={syncingAll}
                onRescan={refreshRepos}
                onRecheck={handleRecheckRepo}
                onSyncRepo={handleSyncRepo}
                onSyncAllDrifted={handleSyncAllDrifted}
                onOpenFolder={handleOpenFolder}
              />
            )}

            {activeTab === "rules" && (
              <RuleEditorView
                rules={rules}
                selectedRuleName={selectedRuleName}
                onSelectRule={setSelectedRuleName}
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
                  setSelectedRuleName("init.md");
                  setActiveTab("rules");
                }}
              />
            )}

            {activeTab === "settings" && appState && (
              <SettingsView
                appState={appState}
                onSaveConfig={handleSaveConfig}
                onRemoveLegacy={handleRemoveLegacy}
                onOpenFolder={handleOpenFolder}
              />
            )}
          </main>
        </div>

        <ToastHost toasts={toasts} onDismiss={dismissToast} />
      </div>
    </>
  );
}

export default App;
