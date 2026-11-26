import { useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import {
  Bot,
  CornerDownLeft,
  FolderOpen,
  FolderSearch,
  LogIn,
  Play,
  RefreshCw,
  Square,
  Trash2,
} from "lucide-react";
import type { AgentRuns } from "../hooks/useAgentRuns";
import type {
  AgentPermission,
  AgentRunPreset,
  AgentSession,
  CliInfo,
  RepoStatus,
  SkillEntry,
  ToastItem,
} from "../types";

interface AgentRunsViewProps {
  runs: AgentRuns;
  clis: CliInfo[];
  skills: SkillEntry[];
  repos: RepoStatus[];
  preset: AgentRunPreset | null;
  onOpenFolder: (path: string) => void;
  onToast: (tone: ToastItem["tone"], title: string, detail?: string) => void;
  footer?: React.ReactNode;
}

type PromptMode = "skill" | "custom";

interface Composer {
  cliId: string;
  repoPath: string;
  mode: PromptMode;
  skill: string;
  extra: string;
  custom: string;
  /** Per CLI, since model names differ between tools. */
  model: Record<string, string>;
  effort: Record<string, string>;
  permission: AgentPermission;
}

const STORAGE_KEY = "brain.agentRuns.composer";
const DEFAULT_SKILL = "gloria-init-brain";

const DEFAULT_COMPOSER: Composer = {
  cliId: "claude",
  repoPath: "",
  mode: "skill",
  skill: DEFAULT_SKILL,
  extra: "",
  custom: "",
  model: {},
  effort: {},
  permission: "edits",
};

function loadComposer(): Composer {
  try {
    const raw = window.localStorage.getItem(STORAGE_KEY);
    if (raw) return { ...DEFAULT_COMPOSER, ...JSON.parse(raw) };
  } catch {
    // Corrupt storage just falls back to defaults.
  }
  return DEFAULT_COMPOSER;
}

/** How each CLI invokes a skill inside a prompt. */
const SKILL_PREFIX: Record<string, string> = { codex: "$" };
const skillPrefix = (cliId: string) => SKILL_PREFIX[cliId] ?? "/";

const PERMISSIONS: Array<{ id: AgentPermission; label: string; hint: string }> = [
  {
    id: "plan",
    label: "Read-only",
    hint: "The agent can read and plan but cannot change files.",
  },
  {
    id: "edits",
    label: "Edit files",
    hint: "File edits are allowed. Shell commands outside the CLI's own allow-list are refused.",
  },
  {
    id: "full",
    label: "Full access",
    hint: "No permission checks at all: edits, shell commands and network. Use only on repos you trust.",
  },
];

const STATUS_TONE: Record<AgentSession["status"], string> = {
  running: "drift",
  done: "ok",
  failed: "error",
  stopped: "muted",
};

/** Output that means the CLI has no usable login. */
const LOGIN_RE = /not logged in|please run \/login|run `?codex login|not signed in/i;

function elapsed(s: AgentSession, now: number) {
  const ms = (s.ended_at ?? now) - s.started_at;
  const sec = Math.max(0, Math.floor(ms / 1000));
  const m = Math.floor(sec / 60);
  return m > 0 ? `${m}m ${String(sec % 60).padStart(2, "0")}s` : `${sec}s`;
}

export function AgentRunsView({
  runs,
  clis,
  skills,
  repos,
  preset,
  onOpenFolder,
  onToast,
  footer,
}: AgentRunsViewProps) {
  const [form, setForm] = useState<Composer>(loadComposer);
  const [activeKey, setActiveKey] = useState<string | null>(runs.sessions[0]?.key ?? null);
  const [replyText, setReplyText] = useState<Record<string, string>>({});
  const [starting, setStarting] = useState(false);
  const [now, setNow] = useState(Date.now());

  const set = <K extends keyof Composer>(k: K, v: Composer[K]) =>
    setForm((f) => ({ ...f, [k]: v }));

  useEffect(() => {
    try {
      window.localStorage.setItem(STORAGE_KEY, JSON.stringify(form));
    } catch {
      // Storage is a convenience only.
    }
  }, [form]);

  // A "Run" click on the Dashboard preselects the repo and the skill.
  const lastPreset = useRef<number | null>(null);
  useEffect(() => {
    if (!preset || preset.nonce === lastPreset.current) return;
    lastPreset.current = preset.nonce;
    setForm((f) => ({ ...f, repoPath: preset.repo_path, mode: "skill", skill: preset.skill }));
  }, [preset]);

  useEffect(() => {
    if (runs.runningCount === 0) return;
    const t = window.setInterval(() => setNow(Date.now()), 1000);
    return () => window.clearInterval(t);
  }, [runs.runningCount]);

  const installed = clis.filter((c) => c.installed);
  const cli = clis.find((c) => c.id === form.cliId) ?? installed[0] ?? null;
  const cliId = cli?.id ?? form.cliId;

  const { catalogs, loadingModels, loadModels } = runs;
  useEffect(() => {
    if (cli?.installed && !catalogs[cli.id] && !loadingModels[cli.id]) {
      void loadModels(cli.id);
    }
  }, [cli, catalogs, loadingModels, loadModels]);

  const { auth, checkingAuth, checkAuth, signIn } = runs;
  useEffect(() => {
    if (cli?.installed && !auth[cli.id] && !checkingAuth[cli.id]) {
      void checkAuth(cli.id);
    }
  }, [cli, auth, checkingAuth, checkAuth]);

  // Sign-in happens in a separate terminal; recheck once the user comes back.
  const authRef = useRef(auth);
  authRef.current = auth;
  useEffect(() => {
    const onFocus = () => {
      Object.values(authRef.current)
        .filter((a) => a.state === "signed_out")
        .forEach((a) => void checkAuth(a.cli_id));
    };
    window.addEventListener("focus", onFocus);
    return () => window.removeEventListener("focus", onFocus);
  }, [checkAuth]);

  const authStatus = auth[cliId];
  const signedOut = authStatus?.state === "signed_out";

  const catalog = catalogs[cliId];
  const model = form.model[cliId] ?? "";
  const effort = form.effort[cliId] ?? "";
  const modelInfo = catalog?.models.find((m) => m.id === model);
  const effortOptions =
    modelInfo && modelInfo.efforts.length > 0 ? modelInfo.efforts : catalog?.efforts ?? [];

  const toolSkills = useMemo(
    () =>
      skills
        .filter((s) => s.tool === cli?.skill_tool)
        .map((s) => s.skill_name)
        .sort(),
    [skills, cli]
  );
  const skill = toolSkills.includes(form.skill) ? form.skill : toolSkills[0] ?? "";

  const repoOptions = useMemo(() => {
    const list = repos
      .filter((r) => r.status !== "ERROR")
      .map((r) => ({ path: r.repo_path, label: r.repo, status: r.status }));
    if (form.repoPath && !list.some((r) => r.path === form.repoPath)) {
      list.unshift({ path: form.repoPath, label: form.repoPath, status: "custom" });
    }
    return list;
  }, [repos, form.repoPath]);

  const prompt =
    form.mode === "skill"
      ? skill
        ? `${skillPrefix(cliId)}${skill}${form.extra.trim() ? ` ${form.extra.trim()}` : ""}`
        : ""
      : form.custom.trim();

  const blocker = !cli
    ? "No CLI detected."
    : !cli.installed
    ? `${cli.name} is not installed.`
    : signedOut
    ? `${cli.name} is not signed in.`
    : !form.repoPath
    ? "Pick a repository."
    : !prompt
    ? form.mode === "skill"
      ? "No Brain skills for this CLI."
      : "Write a prompt."
    : null;

  const browseRepo = async () => {
    try {
      const picked = await open({ directory: true, multiple: false, defaultPath: form.repoPath || undefined });
      if (typeof picked === "string") set("repoPath", picked);
    } catch (err) {
      onToast("error", "Could not open the folder picker", String(err));
    }
  };

  const handleRun = async () => {
    if (blocker || !cli) return;
    setStarting(true);
    try {
      const key = await runs.start({
        cli_id: cli.id,
        cli_name: cli.name,
        repo_path: form.repoPath,
        title: prompt.length > 80 ? `${prompt.slice(0, 80)}…` : prompt,
        prompt,
        model: model || null,
        effort: effort || null,
        permission: form.permission,
      });
      setActiveKey(key);
    } finally {
      setStarting(false);
    }
  };

  const active = runs.sessions.find((s) => s.key === activeKey) ?? runs.sessions[0] ?? null;
  const activeNeedsLogin = useMemo(
    () => !!active && active.status === "failed" && active.lines.some((l) => LOGIN_RE.test(l.text)),
    [active]
  );

  return (
    <div className="view-scroll">
      <div className="page-header">
        <div>
          <div className="page-eyebrow">AGENT RUNS // HEADLESS CLI</div>
          <h1 className="page-title">Agent Runs</h1>
          <p className="page-subtitle">
            Run a Brain skill or any prompt with Claude Code, Codex or Antigravity CLI inside a
            repository, with the model and effort you choose. Runs keep going while you use other
            tabs, and you can reply to continue the same conversation.
          </p>
        </div>
      </div>

      <section className="settings-card run-composer" aria-label="New run">
        <div className="run-field">
          <span className="run-label">CLI</span>
          <div className="pill-group" role="radiogroup" aria-label="CLI">
            {clis.map((c) => (
              <button
                key={c.id}
                type="button"
                role="radio"
                aria-checked={cliId === c.id}
                className={`filter-pill ${cliId === c.id ? "active" : ""}`}
                onClick={() => set("cliId", c.id)}
                disabled={!c.installed}
                title={c.installed ? c.exe_path ?? c.name : `${c.name} is not installed`}
              >
                {c.name}
              </button>
            ))}
          </div>
          {cli?.installed && (
            <div className={`run-auth ${signedOut ? "warn" : ""}`}>
              <span className="run-auth-text">
                {checkingAuth[cliId] && !authStatus
                  ? "Checking sign-in…"
                  : authStatus?.state === "signed_in"
                  ? `Signed in${authStatus.detail ? ` ${authStatus.detail}` : ""}`
                  : signedOut
                  ? `${cli.name} is not signed in. Brain runs the CLI itself, so it needs its own login.`
                  : authStatus?.detail ||
                    `${cli.name} signs in on its first interactive start.`}
              </span>
              {authStatus && authStatus.state !== "signed_in" && (
                <button
                  type="button"
                  className={`btn btn-sm ${signedOut ? "btn-primary" : ""}`}
                  onClick={() => void signIn(cliId, cli.name)}
                  title="Opens a terminal running the CLI's own sign-in"
                >
                  <LogIn size={12} />
                  <span>Sign in</span>
                </button>
              )}
              <button
                type="button"
                className="icon-btn"
                onClick={() => void checkAuth(cliId)}
                disabled={checkingAuth[cliId]}
                title="Check sign-in again"
                aria-label="Check sign-in again"
              >
                <RefreshCw size={11} className={checkingAuth[cliId] ? "spin" : ""} />
              </button>
            </div>
          )}
        </div>

        <div className="run-field">
          <span className="run-label">Repository</span>
          <div className="run-inline">
            <select
              className="select"
              value={form.repoPath}
              onChange={(e) => set("repoPath", e.target.value)}
              aria-label="Repository"
            >
              <option value="">Select a repository…</option>
              {repoOptions.map((r) => (
                <option key={r.path} value={r.path}>
                  {r.label}
                  {r.status !== "UP TO DATE" && r.status !== "custom" ? `  ·  ${r.status}` : ""}
                </option>
              ))}
            </select>
            <button type="button" className="btn btn-sm" onClick={() => void browseRepo()}>
              <FolderSearch size={12} />
              <span>Browse</span>
            </button>
          </div>
        </div>

        <div className="run-field run-field-wide">
          <span className="run-label">Prompt</span>
          <div className="pill-group" role="radiogroup" aria-label="Prompt type">
            {(["skill", "custom"] as const).map((m) => (
              <button
                key={m}
                type="button"
                role="radio"
                aria-checked={form.mode === m}
                className={`filter-pill ${form.mode === m ? "active" : ""}`}
                onClick={() => set("mode", m)}
              >
                {m === "skill" ? "Brain skill" : "Custom prompt"}
              </button>
            ))}
          </div>
          {form.mode === "skill" ? (
            <div className="run-inline">
              <select
                className="select"
                value={skill}
                onChange={(e) => set("skill", e.target.value)}
                aria-label="Skill"
                disabled={toolSkills.length === 0}
              >
                {toolSkills.length === 0 && <option value="">No skills for this CLI</option>}
                {toolSkills.map((s) => (
                  <option key={s} value={s}>
                    {skillPrefix(cliId)}
                    {s}
                  </option>
                ))}
              </select>
              <input
                className="input"
                value={form.extra}
                onChange={(e) => set("extra", e.target.value)}
                placeholder="Optional arguments"
                aria-label="Skill arguments"
              />
            </div>
          ) : (
            <textarea
              className="textarea run-textarea"
              value={form.custom}
              onChange={(e) => set("custom", e.target.value)}
              placeholder="What should the agent do in this repo?"
              aria-label="Prompt"
              rows={3}
            />
          )}
        </div>

        <div className="run-field">
          <span className="run-label">
            Model
            <button
              type="button"
              className="icon-btn"
              onClick={() => void loadModels(cliId, true)}
              disabled={!cli?.installed || loadingModels[cliId]}
              title={catalog ? `Reload (source: ${catalog.source})` : "Load models"}
              aria-label="Reload models"
            >
              <RefreshCw size={11} className={loadingModels[cliId] ? "spin" : ""} />
            </button>
          </span>
          <select
            className="select"
            value={model}
            onChange={(e) => setForm((f) => ({ ...f, model: { ...f.model, [cliId]: e.target.value } }))}
            aria-label="Model"
            disabled={!cli?.installed}
          >
            <option value="">{loadingModels[cliId] ? "Loading models…" : "CLI default"}</option>
            {catalog?.models.map((m) => (
              <option key={m.id} value={m.id}>
                {m.label === m.id ? m.id : `${m.label} (${m.id})`}
              </option>
            ))}
            {model && !catalog?.models.some((m) => m.id === model) && (
              <option value={model}>{model}</option>
            )}
          </select>
          {catalog?.error && <span className="run-note warn">{catalog.error}</span>}
        </div>

        <div className="run-field">
          <span className="run-label">Effort</span>
          <select
            className="select"
            value={effort}
            onChange={(e) => setForm((f) => ({ ...f, effort: { ...f.effort, [cliId]: e.target.value } }))}
            aria-label="Effort"
            disabled={!cli?.installed}
          >
            <option value="">CLI default</option>
            {effortOptions.map((e) => (
              <option key={e} value={e}>
                {e}
              </option>
            ))}
            {effort && !effortOptions.includes(effort) && <option value={effort}>{effort}</option>}
          </select>
        </div>

        <div className="run-field run-field-wide">
          <span className="run-label">Permissions</span>
          <div className="pill-group" role="radiogroup" aria-label="Permissions">
            {PERMISSIONS.map((p) => (
              <button
                key={p.id}
                type="button"
                role="radio"
                aria-checked={form.permission === p.id}
                className={`filter-pill ${form.permission === p.id ? "active" : ""}`}
                onClick={() => set("permission", p.id)}
                title={p.hint}
              >
                {p.label}
              </button>
            ))}
          </div>
          <span className={`run-note ${form.permission === "full" ? "warn" : ""}`}>
            {PERMISSIONS.find((p) => p.id === form.permission)?.hint}
          </span>
        </div>

        <div className="run-submit">
          <code className="run-preview selectable" title={prompt}>
            {prompt || "—"}
          </code>
          <button
            type="button"
            className="btn btn-primary"
            onClick={() => void handleRun()}
            disabled={!!blocker || starting}
            title={blocker ?? `Run in ${form.repoPath}`}
          >
            <Play size={14} />
            <span>{starting ? "Starting..." : "Run"}</span>
          </button>
        </div>
        {blocker && <div className="run-note">{blocker}</div>}
      </section>

      {runs.sessions.length === 0 ? (
        <div className="empty-state">No runs yet. Pick a repository and press Run.</div>
      ) : (
        <section className="run-sessions" aria-label="Runs">
          <div className="run-tabs" role="tablist" aria-label="Runs">
            {runs.sessions.map((s) => (
              <button
                key={s.key}
                type="button"
                role="tab"
                aria-selected={active?.key === s.key}
                className={`run-tab ${active?.key === s.key ? "active" : ""}`}
                onClick={() => setActiveKey(s.key)}
              >
                <span className={`run-dot ${STATUS_TONE[s.status]}`} />
                <span className="run-tab-name">{s.repo_name}</span>
                <span className="run-tab-meta">{s.cli_name}</span>
              </button>
            ))}
          </div>
          {active && (
            <SessionPanel
              key={active.key}
              s={active}
              now={now}
              reply={replyText[active.key] ?? ""}
              onReplyChange={(v) => setReplyText((r) => ({ ...r, [active.key]: v }))}
              onReply={async (permission) => {
                const text = (replyText[active.key] ?? "").trim();
                if (!text) return;
                if (await runs.reply(active.key, text, permission)) {
                  setReplyText((r) => ({ ...r, [active.key]: "" }));
                }
              }}
              onStop={() => void runs.stop(active.key)}
              onRemove={() => {
                runs.remove(active.key);
                setActiveKey(null);
              }}
              onOpenFolder={onOpenFolder}
              onSignIn={
                activeNeedsLogin && auth[active.cli_id]?.state !== "signed_in"
                  ? () => void signIn(active.cli_id, active.cli_name)
                  : undefined
              }
            />
          )}
        </section>
      )}
      {footer}
    </div>
  );
}

interface SessionPanelProps {
  s: AgentSession;
  now: number;
  reply: string;
  onReplyChange: (v: string) => void;
  onReply: (permission: AgentPermission) => void;
  onStop: () => void;
  onRemove: () => void;
  onOpenFolder: (path: string) => void;
  /** Set when the run failed because the CLI is not signed in. */
  onSignIn?: () => void;
}

function SessionPanel({
  s,
  now,
  reply,
  onReplyChange,
  onReply,
  onStop,
  onRemove,
  onOpenFolder,
  onSignIn,
}: SessionPanelProps) {
  const logRef = useRef<HTMLDivElement>(null);
  const stick = useRef(true);
  const [permission, setPermission] = useState<AgentPermission>(s.permission);
  const running = s.status === "running";

  useLayoutEffect(() => {
    const el = logRef.current;
    if (el && stick.current) el.scrollTop = el.scrollHeight;
  }, [s.lines.length]);

  return (
    <div className="settings-card run-panel">
      <div className="run-panel-head">
        <div style={{ minWidth: 0 }}>
          <div className="run-panel-title">
            <Bot size={15} />
            <span className="selectable">{s.title}</span>
          </div>
          <div className="run-panel-meta">
            <span className={`status-badge ${STATUS_TONE[s.status]}`}>
              {s.status === "running" && <RefreshCw size={10} className="spin" />}
              {s.status.toUpperCase()}
            </span>
            <span>{s.cli_name}</span>
            <span>{s.model ?? "default model"}</span>
            <span>{s.effort ? `${s.effort} effort` : "default effort"}</span>
            <span>{PERMISSIONS.find((p) => p.id === s.permission)?.label}</span>
            <span>{elapsed(s, now)}</span>
            {s.turns > 1 && <span>{s.turns} turns</span>}
            {s.session_id && (
              <span className="selectable" title="Conversation id">
                {s.session_id.slice(0, 8)}
              </span>
            )}
          </div>
          <div className="cli-path selectable" title={s.repo_path}>
            {s.repo_path}
          </div>
        </div>
        <div className="run-panel-actions">
          <button
            type="button"
            className="btn btn-sm"
            onClick={() => onOpenFolder(s.repo_path)}
            aria-label="Open repository folder"
            title="Open repository folder"
          >
            <FolderOpen size={12} />
          </button>
          {running ? (
            <button type="button" className="btn btn-sm btn-danger" onClick={onStop}>
              <Square size={11} />
              <span>Stop</span>
            </button>
          ) : (
            <button type="button" className="btn btn-sm" onClick={onRemove} title="Remove from the list">
              <Trash2 size={12} />
              <span>Remove</span>
            </button>
          )}
        </div>
      </div>

      <div
        ref={logRef}
        className="run-log selectable"
        onScroll={(e) => {
          const el = e.currentTarget;
          stick.current = el.scrollHeight - el.scrollTop - el.clientHeight < 40;
        }}
        role="log"
        aria-live="polite"
      >
        {s.dropped > 0 && <div className="run-line info">… {s.dropped} earlier lines hidden</div>}
        {s.lines.map((l) => (
          <div key={l.id} className={`run-line ${l.stream}`}>
            {l.stream === "you" ? `› ${l.text}` : l.text}
          </div>
        ))}
        {running && s.lines.length === 0 && <div className="run-line info">Starting…</div>}
      </div>

      {onSignIn && (
        <div className="run-auth warn">
          <span className="run-auth-text">
            {s.cli_name} is not signed in. Sign in once, then start the run again.
          </span>
          <button type="button" className="btn btn-sm btn-primary" onClick={onSignIn}>
            <LogIn size={12} />
            <span>Sign in</span>
          </button>
        </div>
      )}

      <form
        className="run-reply"
        onSubmit={(e) => {
          e.preventDefault();
          if (!running) onReply(permission);
        }}
      >
        <input
          className="input"
          value={reply}
          onChange={(e) => onReplyChange(e.target.value)}
          placeholder={running ? "Wait for this turn to finish to reply" : 'Reply to continue, for example "apply"'}
          disabled={running}
          aria-label="Reply"
        />
        <select
          className="select run-reply-perm"
          value={permission}
          onChange={(e) => setPermission(e.target.value as AgentPermission)}
          disabled={running}
          aria-label="Permissions for the reply"
          title={PERMISSIONS.find((p) => p.id === permission)?.hint}
        >
          {PERMISSIONS.map((p) => (
            <option key={p.id} value={p.id}>
              {p.label}
            </option>
          ))}
        </select>
        <button type="submit" className="btn btn-sm btn-primary" disabled={running || !reply.trim()}>
          <CornerDownLeft size={12} />
          <span>Send</span>
        </button>
      </form>
    </div>
  );
}
