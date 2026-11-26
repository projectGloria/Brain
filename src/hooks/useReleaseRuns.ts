import { useCallback, useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { api } from "../api";
import type {
  ReleaseDoneEvent,
  ReleaseJob,
  ReleaseLogEvent,
  ReleaseLogLine,
  ReleaseStepEvent,
  ReleaseStepState,
  ToastItem,
} from "../types";

const MAX_LINES = 4000;
const FLUSH_MS = 80;

type Toast = (tone: ToastItem["tone"], title: string, detail?: string) => void;

const repoName = (path: string) => path.replace(/[\\/]+$/, "").split(/[\\/]/).pop() || path;

const INITIAL_STEPS: Array<{ id: ReleaseStepState["id"]; label: string }> = [
  { id: "preflight", label: "Preflight" },
  { id: "bump", label: "Bump" },
  { id: "validate", label: "Validate" },
  { id: "push", label: "Push" },
  { id: "dispatch", label: "Dispatch" },
  { id: "watch", label: "Watch CI" },
  { id: "report", label: "Live" },
];

function createSteps(): ReleaseStepState[] {
  return INITIAL_STEPS.map((s) => ({
    id: s.id,
    label: s.label,
    status: "pending",
    detail: null,
  }));
}

export function useReleaseRuns(onToast: Toast) {
  const [jobsByRepo, setJobsByRepo] = useState<Record<string, ReleaseJob>>({});

  const runToRepo = useRef(new Map<string, string>());
  const pending = useRef(new Map<string, ReleaseLogLine[]>());
  const flushTimer = useRef<number | null>(null);
  const lineId = useRef(1);
  const seq = useRef(1);
  const toastRef = useRef(onToast);
  toastRef.current = onToast;
  const jobsRef = useRef(jobsByRepo);
  jobsRef.current = jobsByRepo;

  const appendLines = (job: ReleaseJob, add: ReleaseLogLine[]): ReleaseJob => {
    const lines = job.lines.concat(add);
    const over = lines.length - MAX_LINES;
    return over > 0
      ? { ...job, lines: lines.slice(over), dropped: job.dropped + over }
      : { ...job, lines };
  };

  const flush = useCallback(() => {
    flushTimer.current = null;
    if (pending.current.size === 0) return;
    const batch = pending.current;
    pending.current = new Map();
    setJobsByRepo((prev) => {
      let changed = false;
      const next = { ...prev };
      for (const [repoPath, add] of batch.entries()) {
        const job = next[repoPath];
        if (job) {
          next[repoPath] = appendLines(job, add);
          changed = true;
        }
      }
      return changed ? next : prev;
    });
  }, []);

  const queueLine = useCallback(
    (repoPath: string, stream: ReleaseLogLine["stream"], text: string) => {
      const list = pending.current.get(repoPath) ?? [];
      list.push({ id: lineId.current++, stream, text });
      pending.current.set(repoPath, list);
      if (flushTimer.current === null) {
        flushTimer.current = window.setTimeout(flush, FLUSH_MS);
      }
    },
    [flush]
  );

  useEffect(() => {
    let disposed = false;
    const unlisten: Array<() => void> = [];
    const keep = (p: Promise<() => void>) =>
      p.then((u) => (disposed ? u() : unlisten.push(u))).catch(() => {});

    keep(
      listen<ReleaseStepEvent>("release-step", ({ payload }) => {
        const repoPath = runToRepo.current.get(payload.run_id);
        if (!repoPath) return;
        setJobsByRepo((prev) => {
          const job = prev[repoPath];
          if (!job || job.run_id !== payload.run_id) return prev;
          return {
            ...prev,
            [repoPath]: {
              ...job,
              steps: job.steps.map((s) =>
                s.id === payload.step_id
                  ? { ...s, status: payload.status, detail: payload.detail ?? s.detail }
                  : s
              ),
            },
          };
        });
      })
    );

    keep(
      listen<ReleaseLogEvent>("release-log", ({ payload }) => {
        const repoPath = runToRepo.current.get(payload.run_id);
        if (repoPath) queueLine(repoPath, payload.stream, payload.text);
      })
    );

    keep(
      listen<ReleaseDoneEvent>("release-done", ({ payload }) => {
        const repoPath = runToRepo.current.get(payload.run_id);
        if (!repoPath) return;
        runToRepo.current.delete(payload.run_id);
        flush();

        const nextStatus: ReleaseJob["status"] = payload.cancelled
          ? "stopped"
          : payload.ok
          ? "done"
          : "failed";

        setJobsByRepo((prev) => {
          const job = prev[repoPath];
          if (!job || job.run_id !== payload.run_id) return prev;
          return {
            ...prev,
            [repoPath]: {
              ...job,
              status: nextStatus,
              ended_at: Date.now(),
              old_version: payload.old_version ?? job.old_version,
              new_version: payload.new_version,
              run_url: payload.run_url ?? job.run_url,
              release_url: payload.release_url ?? job.release_url,
              assets: payload.assets,
              error: payload.error,
            },
          };
        });

        const name = repoName(repoPath);
        if (nextStatus === "done") {
          toastRef.current(
            "success",
            `Released ${name} v${payload.new_version}`,
            payload.release_url ?? "GitHub Release is live."
          );
        } else if (nextStatus === "failed") {
          toastRef.current(
            "error",
            `Release failed for ${name} v${payload.new_version}`,
            payload.error ?? "Check the release log in DNA."
          );
        }
      })
    );

    return () => {
      disposed = true;
      unlisten.forEach((u) => u());
    };
  }, [flush, queueLine]);

  const start = useCallback(
    async (repoPath: string, version: string, oldVersion: string | null, note?: string) => {
      const runId = `rel-${Date.now().toString(36)}-${seq.current++}`;
      runToRepo.current.set(runId, repoPath);

      const job: ReleaseJob = {
        run_id: runId,
        repo_path: repoPath,
        repo_name: repoName(repoPath),
        old_version: oldVersion,
        new_version: version,
        status: "running",
        started_at: Date.now(),
        ended_at: null,
        steps: createSteps(),
        lines: [],
        dropped: 0,
        run_url: null,
        release_url: null,
        assets: [],
        error: null,
      };
      setJobsByRepo((prev) => ({ ...prev, [repoPath]: job }));

      try {
        const started = await api.startRelease({
          run_id: runId,
          repo_path: repoPath,
          version,
          note: note?.trim() || null,
        });
        setJobsByRepo((prev) => {
          const cur = prev[repoPath];
          if (!cur || cur.run_id !== runId) return prev;
          return { ...prev, [repoPath]: { ...cur, new_version: started.version } };
        });
        return true;
      } catch (err) {
        runToRepo.current.delete(runId);
        const msg = String(err);
        setJobsByRepo((prev) => {
          const cur = prev[repoPath];
          if (!cur || cur.run_id !== runId) return prev;
          return {
            ...prev,
            [repoPath]: {
              ...cur,
              status: "failed",
              ended_at: Date.now(),
              error: msg,
            },
          };
        });
        toastRef.current("error", "Could not start release", msg);
        return false;
      }
    },
    []
  );

  const cancel = useCallback(async (repoPath: string) => {
    const job = jobsRef.current[repoPath];
    if (!job || job.status !== "running") return;
    try {
      await api.cancelRelease(job.run_id);
    } catch (err) {
      toastRef.current("error", "Could not stop release", String(err));
    }
  }, []);

  const clear = useCallback((repoPath: string) => {
    setJobsByRepo((prev) => {
      const job = prev[repoPath];
      if (!job || job.status === "running") return prev;
      const next = { ...prev };
      delete next[repoPath];
      return next;
    });
  }, []);

  return {
    jobsByRepo,
    start,
    cancel,
    clear,
  };
}

export type ReleaseRuns = ReturnType<typeof useReleaseRuns>;
