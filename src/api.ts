import { invoke } from "@tauri-apps/api/core";
import type {
  AppConfig,
  AppStateSnapshot,
  FingerprintPreview,
  InstallResult,
  RepoStatus,
  RuleDocument,
  RuleSummary,
  SkillEntry,
  SyncResult,
} from "./types";

export const api = {
  getAppState: () => invoke<AppStateSnapshot>("get_app_state"),
  updateConfig: (config: AppConfig) =>
    invoke<AppStateSnapshot>("update_config", { config }),
  scanRepos: () => invoke<RepoStatus[]>("scan_repos"),
  checkRepo: (repoPath: string) =>
    invoke<RepoStatus>("check_repo_cmd", { repoPath }),
  syncRepo: (repoPath: string) =>
    invoke<SyncResult>("sync_repo_cmd", { repoPath }),
  syncAllRepos: (repoPaths: string[]) =>
    invoke<SyncResult[]>("sync_all_repos_cmd", { repoPaths }),
  installBrain: () => invoke<InstallResult>("install_brain_cmd"),
  removeLegacySkill: () => invoke<boolean>("remove_legacy_skill_cmd"),
  listRules: () => invoke<RuleSummary[]>("list_rules_cmd"),
  readRule: (name: string) => invoke<RuleDocument>("read_rule_cmd", { name }),
  saveRule: (name: string, rawContent: string) =>
    invoke<RuleDocument>("save_rule_cmd", { name, rawContent }),
  createRule: (name: string) =>
    invoke<RuleDocument>("create_rule_cmd", { name }),
  deleteRule: (name: string) => invoke<void>("delete_rule_cmd", { name }),
  previewFingerprint: (fileName: string | null, draftContent: string | null) =>
    invoke<FingerprintPreview>("preview_fingerprint_cmd", {
      fileName,
      draftContent,
    }),
  listSkills: () => invoke<SkillEntry[]>("list_skills_cmd"),
  saveSkill: (tool: string, skillName: string, content: string) =>
    invoke<SkillEntry>("save_skill_cmd", { tool, skillName, content }),
  openInExplorer: (path: string) =>
    invoke<void>("open_in_explorer_cmd", { path }),
};
