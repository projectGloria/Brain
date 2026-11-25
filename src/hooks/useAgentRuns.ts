import { useCallback, useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { api } from "../api";
import type {
  AgentExitEvent,
  AgentLogLine,
  AgentOutputEvent,
  AgentPermission,
  AgentSession,
  AgentSessionEvent,
  CliAuthStatus,
  ModelCatalog,
  ToastItem,
} from "../types";

/** Older lines are dropped past this so a chatty agent can't bloat the UI. */
const MAX_LINES = 5000;
const FLUSH_MS = 80;

export interface StartRunOptions {
  cli_id: string;
  cli_name: string;
  repo_path: string;
  title: string;
  prompt: string;
  model: string | null;
  effort: string | null;
  permission: AgentPermission;
}

type Toast = (tone: ToastItem["tone"], title: string, detail?: string) => void;

const repoName = (path: string) => path.replace(/[\\/]+$/, "").split(/[\\/]/).pop() || path;

/**
 * Agent runs live in App (not in the view) so a running agent keeps streaming
 * while the user is on another tab.
 */
export function useAgentRuns(onToast: Toast) {
  const [sessions, setSessions] = useState<AgentSession[]>([]);
  const [catalogs, setCatalogs] = useState<Record<string, ModelCatalog>>({});
  const [loadingModels, setLoadingModels] = useState<Record<string, boolean>>({});
  const [auth, setAuth] = useState<Record<string, CliAuthStatus>>({});
  const [checkingAuth, setCheckingAuth] = useState<Record<string, boolean>>({});

  const runToKey = useRef(new Map<string, string>());
  const pending = useRef(new Map<string, AgentLogLine[]>());
  const flushTimer = useRef<number | null>(null);
  const lineId = useRef(1);
  const seq = useRef(1);
  const toastRef = useRef(onToast);
  toastRef.current = onToast;
  const sessionsRef = useRef(sessions);
  sessionsRef.current = sessions;

  const patch = useCallback((key: string, fn: (s: AgentSession) => AgentSession) => {
    setSessions((prev) => prev.map((s) => (s.key === key ? fn(s) : s)));
  }, []);

  const appendLines = (s: AgentSession, add: AgentLogLine[]): AgentSession => {
    const lines = s.lines.concat(add);
    const over = lines.length - MAX_LINES;
    return over > 0
      ? { ...s, lines: lines.slice(over), dropped: s.dropped + over }
      : { ...s, lines };
  };

  const flush = useCallback(() => {
    flushTimer.current = null;
    if (pending.current.size === 0) return;
    const batch = pending.current;
    pending.current = new Map();
    setSessions((prev) =>
      prev.map((s) => {
        const add = batch.get(s.key);
        return add ? appendLines(s, add) : s;
      })
    );
  }, []);

  const queueLine = useCallback(
    (key: string, stream: AgentLogLine["stream"], text: string) => {
      const list = pending.current.get(key) ?? [];
      list.push({ id: lineId.current++, stream, text });
      pending.current.set(key, list);
      if (flushTimer.current === null) {
        flushTimer.current = window.setTimeout(flush, FLUSH_MS);
      }
    },
    [flush]
  );

  const checkAuth = useCallback(async (cliId: string) => {
    setCheckingAuth((m) => ({ ...m, [cliId]: true }));
    try {
      const status = await api.cliAuthStatus(cliId);
      setAuth((a) => ({ ...a, [cliId]: status }));
      return status;
    } catch (err) {
      const status: CliAuthStatus = { cli_id: cliId, state: "unknown", detail: String(err) };
      setAuth((a) => ({ ...a, [cliId]: status }));
      return status;
    } finally {
      setCheckingAuth((m) => ({ ...m, [cliId]: false }));
    }
  }, []);

  /** Opens a terminal running the CLI's own sign-in; the user finishes it there. */
  const signIn = useCallback(async (cliId: string, cliName: string) => {
    try {
      await api.openCliLogin(cliId);
      toastRef.current(
        "info",
        `Sign in to ${cliName} in the terminal window`,
        "Brain checks again when you come back to this window."
      );
    } catch (err) {
      toastRef.current("error", `Could not start the ${cliName} sign-in`, String(err));
    }
  }, []);

  useEffect(() => {
    let disposed = false;
    const unlisten: Array<() => void> = [];
    const keep = (p: Promise<() => void>) =>
      p.then((u) => (disposed ? u() : unlisten.push(u))).catch(() => {});

    keep(
      listen<AgentOutputEvent>("agent-run-output", ({ payload }) => {
        const key = runToKey.current.get(payload.run_id);
        if (key) queueLine(key, payload.stream, payload.text);
      })
    );
    keep(
      listen<AgentSessionEvent>("agent-run-session", ({ payload }) => {
        const key = runToKey.current.get(payload.run_id);
        if (key) patch(key, (s) => ({ ...s, session_id: payload.session_id }));
      })
    );
    keep(
      listen<AgentExitEvent>("agent-run-exit", ({ payload }) => {
        const key = runToKey.current.get(payload.run_id);
        if (!key) return;
        runToKey.current.delete(payload.run_id);
        flush();
        const status = payload.cancelled
          ? "stopped"
          : payload.exit_code === 0 && !payload.error
          ? "done"
          : "failed";
        if (payload.error) queueLine(key, "err", payload.error);
        queueLine(
          key,
          "info",
          status === "stopped" ? "■ stopped" : `■ exited with code ${payload.exit_code ?? "?"}`
        );
        patch(key, (s) => ({
          ...s,
          status,
          run_id: null,
          exit_code: payload.exit_code,
          ended_at: Date.now(),
        }));
        const s = sessionsRef.current.find((x) => x.key === key);
        // A failed run is often a signed-out CLI; refresh so the composer can say so.
        if (s && status === "failed") void checkAuth(s.cli_id);
        if (s && status !== "stopped") {
          toastRef.current(
            status === "done" ? "success" : "error",
            `${s.cli_name} ${status === "done" ? "finished" : "failed"} in ${s.repo_name}`,
            status === "done"
              ? "Open Agent Runs to read the result or reply."
              : `Exit code ${payload.exit_code ?? "?"}. See the log in Agent Runs.`
          );
        }
      })
    );
    return () => {
      disposed = true;
      unlisten.forEach((u) => u());
    };
  }, [checkAuth, flush, patch, queueLine]);

  const launch = useCallback(
    async (key: string, prompt: string, resumeId: string | null, s: AgentSession) => {
      const runId = `${key}:${s.turns + 1}`;
      runToKey.current.set(runId, key);
      patch(key, (x) =>
        appendLines(
          {
            ...x,
            run_id: runId,
            status: "running",
            turns: x.turns + 1,
            exit_code: null,
            ended_at: null,
          },
          x.turns > 0 ? [{ id: lineId.current++, stream: "you", text: prompt }] : []
        )
      );
      try {
        const started = await api.startAgentRun({
          run_id: runId,
          cli_id: s.cli_id,
          repo_path: s.repo_path,
          prompt,
          model: s.model,
          effort: s.effort,
          permission: s.permission,
          session_id: resumeId,
        });
        if (started.session_id) {
          patch(key, (x) => ({ ...x, session_id: x.session_id ?? started.session_id }));
        }
        return true;
      } catch (err) {
        runToKey.current.delete(runId);
        queueLine(key, "err", String(err));
        patch(key, (x) => ({ ...x, status: "failed", run_id: null, ended_at: Date.now() }));
        toastRef.current("error", `Could not start ${s.cli_name}`, String(err));
        return false;
      }
    },
    [patch, queueLine]
  );

  const start = useCallback(
    async (o: StartRunOptions) => {
      const key = `run-${Date.now().toString(36)}-${seq.current++}`;
      const session: AgentSession = {
        key,
        cli_id: o.cli_id,
        cli_name: o.cli_name,
        repo_path: o.repo_path,
        repo_name: repoName(o.repo_path),
        title: o.title,
        model: o.model,
        effort: o.effort,
        permission: o.permission,
        session_id: null,
        turns: 0,
        run_id: null,
        status: "running",
        exit_code: null,
        started_at: Date.now(),
        ended_at: null,
        lines: [],
        dropped: 0,
      };
      setSessions((prev) => [session, ...prev]);
      await launch(key, o.prompt, null, session);
      return key;
    },
    [launch]
  );

  /** Sends a follow-up turn (for example "apply") into the same conversation. */
  const reply = useCallback(
    async (key: string, prompt: string, permission?: AgentPermission) => {
      const s = sessionsRef.current.find((x) => x.key === key);
      if (!s || s.status === "running") return false;
      const next = permission ? { ...s, permission } : s;
      if (permission) patch(key, (x) => ({ ...x, permission }));
      return launch(key, prompt, s.session_id ?? "last", next);
    },
    [launch, patch]
  );

  const stop = useCallback(
    async (key: string) => {
      const s = sessionsRef.current.find((x) => x.key === key);
      if (!s?.run_id) return;
      try {
        await api.cancelAgentRun(s.run_id);
      } catch (err) {
        toastRef.current("error", "Could not stop the run", String(err));
      }
    },
    []
  );

  const remove = useCallback((key: string) => {
    setSessions((prev) => prev.filter((s) => s.key !== key || s.status === "running"));
  }, []);

  const loadModels = useCallback(async (cliId: string, force = false) => {
    setLoadingModels((m) => ({ ...m, [cliId]: true }));
    try {
      const cat = await api.cliModels(cliId, force);
      setCatalogs((c) => ({ ...c, [cliId]: cat }));
    } catch (err) {
      setCatalogs((c) => ({
        ...c,
        [cliId]: { cli_id: cliId, models: [], efforts: [], source: "-", error: String(err) },
      }));
    } finally {
      setLoadingModels((m) => ({ ...m, [cliId]: false }));
    }
  }, []);

  const runningCount = sessions.filter((s) => s.status === "running").length;

  return {
    sessions,
    runningCount,
    catalogs,
    loadingModels,
    loadModels,
    auth,
    checkingAuth,
    checkAuth,
    signIn,
    start,
    reply,
    stop,
    remove,
  };
}

export type AgentRuns = ReturnType<typeof useAgentRuns>;
