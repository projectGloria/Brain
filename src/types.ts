export type NavTab = "dashboard" | "rules" | "skills" | "settings";

export interface AppConfig {
  brain_repo_path: string | null;
  scan_roots: string[];
  custom_repos: string[];
}

export interface ToolPathInfo {
  tool: string;
  label: string;
  path: string;
  exists: boolean;
}

export interface AppStateSnapshot {
  config: AppConfig;
  master_dir: string;
  master_exists: boolean;
  brain_dir: string | null;
  master_fingerprint: string;
  repo_fingerprint: string;
  master_in_sync_with_repo: boolean;
  stub_files: string[];
  legacy_skill_present: boolean;
  legacy_skill_path: string;
  tool_paths: ToolPathInfo[];
}

export type RepoSyncStatus =
  | "UP TO DATE"
  | "BRAIN DRIFT"
  | "REPO DRIFT"
  | "OVER BUDGET"
  | "NO MARKER"
  | "NO AGENTS.MD";

export interface RepoStatus {
  repo: string;
  repo_path: string;
  status: RepoSyncStatus | string;
  lines_count: number | null;
  lines_display: string;
  sync_marker: string;
  expected: string;
  message: string;
  drifted_files: string[];
  has_project_brain: boolean;
  has_claude_md: boolean;
  local_playbooks: string[];
  is_git_repo: boolean;
  is_brain_repo: boolean;
}

export interface SyncResult {
  repo_path: string;
  copied: string[];
  pruned: string[];
  git_excluded: boolean;
  fingerprint: string;
  marker: string;
}

export interface InstallResult {
  compiled_skills: number;
  master_files_count: number;
  pruned_master: string[];
  installed_skills_count: number;
  fingerprint: string;
  marker: string;
  stub_files: string[];
  legacy_skill_present: boolean;
}

export interface RuleSummary {
  name: string;
  path: string;
  is_init: boolean;
  apply_when: string[];
  mode: string;
  trigger: string | null;
  line_count: number;
  stubs: string[];
  has_non_negotiable_marker: boolean;
}

export interface RuleDocument extends RuleSummary {
  raw_content: string;
  body: string;
}

export interface FingerprintPreview {
  saved_fingerprint: string;
  draft_fingerprint: string;
  changed: boolean;
  stubs: string[];
  has_non_negotiable_marker: boolean;
}

export type SkillSyncStatus = "SYNCED" | "OUT_OF_SYNC" | "NOT_INSTALLED";

export interface SkillEntry {
  tool: "claude-code" | "codex" | "antigravity" | string;
  tool_label: string;
  skill_name: string;
  is_generated: boolean;
  repo_path: string;
  installed_path: string;
  status: SkillSyncStatus | string;
  content: string;
}

export interface ToastItem {
  id: number;
  tone: "success" | "info" | "warning" | "error";
  title: string;
  detail?: string;
}
