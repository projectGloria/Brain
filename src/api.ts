import { invoke } from "@tauri-apps/api/core";
import type {
  AgentRunRequest,
  AgentRunStarted,
  AppConfig,
  AppStateSnapshot,
  BatchSyncItem,
  CliActionResult,
  CliConfigDocument,
  CliInfo,
  CliLatest,
  FingerprintPreview,
  GitignoreApplyResult,
  GitignoreRepoStatus,
  GitignoreTemplate,
  InstallResult,
  ModelCatalog,
  CliAuthStatus,
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
    invoke<BatchSyncItem[]>("sync_all_repos_cmd", { repoPaths }),
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

  readGitignoreTemplate: () =>
    invoke<GitignoreTemplate>("read_gitignore_template_cmd"),
  saveGitignoreTemplate: (content: string) =>
    invoke<GitignoreTemplate>("save_gitignore_template_cmd", { content }),
  gitignoreStatus: (repoPath: string) =>
    invoke<GitignoreRepoStatus>("gitignore_status_cmd", { repoPath }),
  applyGitignore: (repoPath: string) =>
    invoke<GitignoreApplyResult>("apply_gitignore_cmd", { repoPath }),
  applyGitignoreMany: (repoPaths: string[]) =>
    invoke<GitignoreApplyResult[]>("apply_gitignore_many_cmd", { repoPaths }),

  listClis: () => invoke<CliInfo[]>("list_clis_cmd"),
  cliLatestVersions: (force = false) =>
    invoke<CliLatest[]>("cli_latest_versions_cmd", { force }),
  installCli: (id: string) => invoke<CliActionResult>("install_cli_cmd", { id }),
  updateCli: (id: string) => invoke<CliActionResult>("update_cli_cmd", { id }),
  readCliConfig: (id: string, key: string) =>
    invoke<CliConfigDocument>("read_cli_config_cmd", { id, key }),
  saveCliConfig: (id: string, key: string, content: string) =>
    invoke<CliConfigDocument>("save_cli_config_cmd", { id, key, content }),

  startAgentRun: (req: AgentRunRequest) =>
    invoke<AgentRunStarted>("start_agent_run", { req }),
  cancelAgentRun: (runId: string) => invoke<boolean>("cancel_agent_run", { runId }),
  cliModels: (id: string, force = false) =>
    invoke<ModelCatalog>("cli_models_cmd", { id, force }),
  cliAuthStatus: (id: string) => invoke<CliAuthStatus>("cli_auth_status", { id }),
  openCliLogin: (id: string) => invoke<void>("open_cli_login", { id }),
};
