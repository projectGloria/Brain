export type NavTab =
  | "dashboard"
  | "rules"
  | "skills"
  | "runs"
  | "gitignore"
  | "dna"
  | "evolve"
  | "settings";

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
  brain_repo_valid: boolean;
  master_fingerprint: string;
  repo_fingerprint: string;
  master_in_sync_with_repo: boolean;
  skills_in_sync: boolean;
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
  | "NO AGENTS.MD"
  | "ERROR";

export type GitignoreState = "MISSING" | "MATCHES" | "DIFFERS" | "NO TEMPLATE" | "-";

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
  project_brain_in_sync: boolean;
  gitignore_status: GitignoreState | string;
}

export interface SyncResult {
  repo_path: string;
  copied: string[];
  pruned: string[];
  git_excluded: boolean;
  fingerprint: string;
  marker: string;
}

export interface BatchSyncItem {
  repo_path: string;
  ok: boolean;
  error: string | null;
  result: SyncResult | null;
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

export interface GitignoreTemplate {
  path: string;
  content: string;
  exists: boolean;
  line_count: number;
}

export interface GitignoreRepoStatus {
  repo_path: string;
  status: GitignoreState | string;
  repo_line_count: number;
  template_line_count: number;
  missing_lines: string[];
  extra_lines: string[];
}

export interface GitignoreApplyResult {
  repo_path: string;
  ok: boolean;
  error: string | null;
  previous_existed: boolean;
}

export interface CliConfigFile {
  key: string;
  label: string;
  path: string;
  format: "json" | "toml" | "md" | string;
  exists: boolean;
}

export interface CliInfo {
  id: string;
  name: string;
  skill_tool: string;
  installed: boolean;
  version: string | null;
  exe_path: string | null;
  source: "native" | "npm" | "winget" | "unknown" | string;
  winget_id: string;
  npm_package: string | null;
  config_dir: string;
  config_dir_exists: boolean;
  skills_dir: string;
  skills_dir_exists: boolean;
  config_files: CliConfigFile[];
  docs_url: string;
  error: string | null;
}

export interface CliLatest {
  id: string;
  latest: string | null;
  source: string;
  error: string | null;
}

export interface CliActionResult {
  id: string;
  ok: boolean;
  exit_code: number | null;
  command: string;
  log: string;
  info: CliInfo;
}

export interface CliConfigDocument {
  id: string;
  key: string;
  path: string;
  format: string;
  content: string;
  exists: boolean;
}

/** "plan" = read-only, "edits" = may edit files, "full" = no permission checks. */
export type AgentPermission = "plan" | "edits" | "full";

export interface AgentRunRequest {
  run_id: string;
  cli_id: string;
  repo_path: string;
  prompt: string;
  model: string | null;
  effort: string | null;
  permission: AgentPermission;
  /** Conversation to continue; "last" = most recent one in the repo folder. */
  session_id: string | null;
}

export interface AgentRunStarted {
  run_id: string;
  command: string;
  session_id: string | null;
}

export interface ModelOption {
  id: string;
  label: string;
  efforts: string[];
}

export interface ModelCatalog {
  cli_id: string;
  models: ModelOption[];
  efforts: string[];
  source: string;
  error: string | null;
}

export interface CliAuthStatus {
  cli_id: string;
  /** "unknown" when the CLI has no status command (Antigravity) or it could not be read. */
  state: "signed_in" | "signed_out" | "unknown";
  detail: string | null;
}

export interface AgentOutputEvent {
  run_id: string;
  stream: "out" | "err" | "info";
  text: string;
}

export interface AgentSessionEvent {
  run_id: string;
  session_id: string;
}

export interface AgentExitEvent {
  run_id: string;
  exit_code: number | null;
  cancelled: boolean;
  error: string | null;
}

export type AgentSessionStatus = "running" | "done" | "failed" | "stopped";

export interface AgentLogLine {
  id: number;
  stream: "out" | "err" | "info" | "you";
  text: string;
}

export interface AgentSession {
  key: string;
  cli_id: string;
  cli_name: string;
  repo_path: string;
  repo_name: string;
  title: string;
  model: string | null;
  effort: string | null;
  permission: AgentPermission;
  session_id: string | null;
  turns: number;
  run_id: string | null;
  status: AgentSessionStatus;
  exit_code: number | null;
  started_at: number;
  ended_at: number | null;
  lines: AgentLogLine[];
  dropped: number;
}

export interface AgentRunPreset {
  repo_path: string;
  skill: string;
  nonce: number;
}

export type DnaToolStatus = "NEW" | "IN_SYNC" | "OUTDATED" | "CUSTOM";
export type DnaProjectStatus = "MISSING" | "READY" | "OUTDATED" | "PARTIAL";

export interface DnaToolPreview {
  file_name: string;
  description: string;
  status: DnaToolStatus | string;
  content: string;
}

export interface DnaProjectSummary {
  repo: string;
  repo_path: string;
  stacks: string[];
  default_port: number | null;
  main_exe: string | null;
  has_gloria_toml: boolean;
  manifests: string[];
  status: DnaProjectStatus | string;
  existing_count: number;
  expected_count: number;
  custom_count: number;
}

export interface DnaProjectInspect {
  summary: DnaProjectSummary;
  tools: DnaToolPreview[];
}

export interface DnaApplyResult {
  repo_path: string;
  ok: boolean;
  written: string[];
  skipped_custom: string[];
  error: string | null;
}

export type ReleaseStepId =
  | "preflight"
  | "bump"
  | "validate"
  | "push"
  | "dispatch"
  | "watch"
  | "report";

export type ReleaseStepStatus = "pending" | "running" | "done" | "failed" | "skipped";

export interface ReleaseVersionFile {
  path: string;
  version: string;
  matches_primary: boolean;
}

export interface ReleasePreflight {
  repo_name: string;
  repo_path: string;
  current_version: string | null;
  suggested_version: string;
  version_files: ReleaseVersionFile[];
  git_branch: string | null;
  dirty_files: string[];
  last_release: string | null;
  existing_tags: string[];
  workflow_status: "ready" | "custom" | "missing" | string;
  has_gloria_toml: boolean;
  is_gloria_setup: boolean;
  gh_available: boolean;
  dev_self_release: boolean;
  has_check_bat: boolean;
  needs_bootstrap: boolean;
  bootstrap_plan: string[];
  blockers: string[];
}

export interface ReleaseRequest {
  run_id: string;
  repo_path: string;
  version: string;
  note?: string | null;
  run_local_check?: boolean;
  retry_same_version?: boolean;
  pre_fix?: string | null;
}

export interface ReleaseFailureDiagnosis {
  category: string;
  summary: string;
  location: string | null;
  fix_action: string | null;
  fix_label: string | null;
}

export interface ReleaseStarted {
  run_id: string;
  version: string;
}

export interface ReleaseStepEvent {
  run_id: string;
  step_id: ReleaseStepId;
  status: ReleaseStepStatus;
  detail: string | null;
}

export interface ReleaseLogEvent {
  run_id: string;
  stream: "out" | "err" | "info";
  text: string;
}

export interface ReleaseDoneEvent {
  run_id: string;
  ok: boolean;
  cancelled: boolean;
  old_version: string | null;
  new_version: string;
  run_url: string | null;
  release_url: string | null;
  assets: string[];
  error: string | null;
  diagnosis: ReleaseFailureDiagnosis | null;
}

export interface ReleaseLogLine {
  id: number;
  stream: "out" | "err" | "info";
  text: string;
}

export interface ReleaseStepState {
  id: ReleaseStepId;
  label: string;
  status: ReleaseStepStatus;
  detail: string | null;
}

export interface ReleaseJob {
  run_id: string;
  repo_path: string;
  repo_name: string;
  old_version: string | null;
  new_version: string;
  status: "running" | "done" | "failed" | "stopped";
  started_at: number;
  ended_at: number | null;
  steps: ReleaseStepState[];
  lines: ReleaseLogLine[];
  dropped: number;
  run_url: string | null;
  release_url: string | null;
  assets: string[];
  error: string | null;
  diagnosis: ReleaseFailureDiagnosis | null;
}
