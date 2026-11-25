import { useEffect, useMemo, useRef, useState } from "react";
import {
  AlertTriangle,
  CheckCircle2,
  FilePlus,
  Hammer,
  Save,
  ShieldAlert,
  Trash2,
} from "lucide-react";
import { api } from "../api";
import { confirmDiscard, sameText, useUnsavedGuard } from "../hooks/useUnsavedGuard";
import type { FingerprintPreview, RuleDocument, RuleSummary } from "../types";

const KNOWN_TRAITS = [
  "always",
  "desktop-gui",
  "windows-app",
  "web-ui",
  "browser-extension",
  "android-app",
  "cli-backend",
  "has-github-remote",
] as const;

interface RuleEditorViewProps {
  rules: RuleSummary[];
  selectedRuleName: string | null;
  onSelectRule: (name: string) => void;
  /** Changes when the Brain folder changes, forcing the open rule to reload. */
  reloadKey: string | null;
  onRulesChanged: () => Promise<void>;
  onInstallAll: () => Promise<void>;
  onToast: (
    tone: "success" | "info" | "warning" | "error",
    title: string,
    detail?: string
  ) => void;
}

const FRONTMATTER_RE = /^---[ \t]*\r?\n[\s\S]*?\r?\n---[ \t]*(?:\r?\n|$)/;

/** Splits a rule into its raw frontmatter block (kept verbatim) and body. */
function splitRaw(raw: string): { fm: string; body: string } {
  const m = raw.match(FRONTMATTER_RE);
  if (!m) return { fm: "", body: raw };
  return { fm: m[0], body: raw.slice(m[0].length) };
}

/**
 * Sets (or removes, when value is null) one `key: value` line inside a raw
 * frontmatter block, leaving every other line, comment and key untouched.
 */
function setFrontmatterKey(fm: string, key: string, value: string | null): string {
  const block = fm || "---\n---\n";
  const eol = block.includes("\r\n") ? "\r\n" : "\n";
  const lines = block.split(/\r?\n/);
  const closeIdx = lines.findIndex((l, i) => i > 0 && /^---[ \t]*$/.test(l));
  const keyRe = new RegExp(`^${key}[ \t]*:`);
  const idx = lines.findIndex((l, i) => i > 0 && i < closeIdx && keyRe.test(l));
  if (value === null) {
    if (idx >= 0) lines.splice(idx, 1);
  } else if (idx >= 0) {
    lines[idx] = `${key}: ${value}`;
  } else {
    lines.splice(closeIdx, 0, `${key}: ${value}`);
  }
  return lines.join(eol);
}

export function RuleEditorView({
  rules,
  selectedRuleName,
  onSelectRule: selectRuleRaw,
  reloadKey,
  onRulesChanged,
  onInstallAll,
  onToast,
}: RuleEditorViewProps) {
  const [doc, setDoc] = useState<RuleDocument | null>(null);
  const [applyWhen, setApplyWhen] = useState<string[]>([]);
  const [mode, setMode] = useState<string>("inline");
  const [trigger, setTrigger] = useState<string>("");
  const [fmRaw, setFmRaw] = useState<string>("");
  const [body, setBody] = useState<string>("");
  const [draftRaw, setDraftRaw] = useState<string>("");
  const previewReq = useRef(0);
  const [preview, setPreview] = useState<FingerprintPreview | null>(null);

  const [creating, setCreating] = useState(false);
  const [newRuleName, setNewRuleName] = useState("");
  const [saving, setSaving] = useState(false);
  const [confirmDelete, setConfirmDelete] = useState(false);

  const activeName = useMemo(() => {
    if (selectedRuleName && rules.some((r) => r.name === selectedRuleName)) {
      return selectedRuleName;
    }
    return rules[0]?.name ?? null;
  }, [rules, selectedRuleName]);

  useEffect(() => {
    if (!activeName) {
      setDoc(null);
      return;
    }
    let cancelled = false;
    setConfirmDelete(false);
    api
      .readRule(activeName)
      .then((loaded) => {
        if (cancelled) return;
        setDoc(loaded);
        setApplyWhen(loaded.apply_when);
        setMode(loaded.mode || "inline");
        setTrigger(loaded.trigger ?? "");
        const parts = splitRaw(loaded.raw_content);
        setFmRaw(parts.fm);
        setBody(parts.body);
        setDraftRaw(loaded.raw_content);
      })
      .catch((err) => {
        if (!cancelled) {
          onToast("error", `Failed to load ${activeName}`, String(err));
        }
      });

    return () => {
      cancelled = true;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [activeName, reloadKey]);

  useEffect(() => {
    if (!doc) {
      setPreview(null);
      return;
    }
    const req = ++previewReq.current;
    const timer = window.setTimeout(() => {
      api
        .previewFingerprint(doc.name, draftRaw)
        .then((p) => {
          // Ignore answers to older drafts that arrive late.
          if (req === previewReq.current) setPreview(p);
        })
        .catch(() => {});
    }, 120);

    return () => window.clearTimeout(timer);
  }, [doc?.name, draftRaw]);

  const isDirty = doc ? !sameText(draftRaw, doc.raw_content) : false;
  useUnsavedGuard("rule-editor", isDirty, doc ? `brain/${doc.name}` : "rule");

  const onSelectRule = (name: string) => {
    if (name !== doc?.name && !confirmDiscard("rule-editor")) return;
    selectRuleRaw(name);
  };

  const hasInitMarker = useMemo(() => {
    if (!doc?.is_init) return true;
    return draftRaw.replace(/\r\n/g, "\n").includes("## Non-Negotiable Rules\n");
  }, [doc?.is_init, draftRaw]);

  /** Rewrites only the frontmatter keys a control changed; the body is untouched. */
  const updateFrontmatter = (
    nextApplyWhen: string[],
    nextMode: string,
    nextTrigger: string
  ) => {
    let fm = fmRaw;
    if (nextApplyWhen.join(",") !== applyWhen.join(",")) {
      fm = setFrontmatterKey(
        fm,
        "apply_when",
        nextApplyWhen.length > 0 ? nextApplyWhen.join(", ") : null
      );
    }
    if (nextMode !== mode) fm = setFrontmatterKey(fm, "mode", nextMode);
    const wantTrigger =
      nextMode === "reference" && nextTrigger.trim() ? nextTrigger.trim() : null;
    const hadTrigger = mode === "reference" && trigger.trim() ? trigger.trim() : null;
    if (wantTrigger !== hadTrigger) fm = setFrontmatterKey(fm, "trigger", wantTrigger);
    setApplyWhen(nextApplyWhen);
    setMode(nextMode);
    setTrigger(nextTrigger);
    setFmRaw(fm);
    setDraftRaw(fm + body);
  };

  const updateBody = (nextBody: string) => {
    setBody(nextBody);
    setDraftRaw(fmRaw + nextBody);
  };

  const toggleTrait = (trait: string) => {
    const exists = applyWhen.includes(trait);
    const next = exists
      ? applyWhen.filter((t) => t !== trait)
      : [...applyWhen, trait];
    updateFrontmatter(next, mode, trigger);
  };

  const handleSave = async (andInstall: boolean) => {
    if (!doc) return;
    if (doc.is_init && !hasInitMarker) {
      onToast(
        "error",
        "Missing required anchor",
        "init.md must keep the '## Non-Negotiable Rules' heading."
      );
      return;
    }
    setSaving(true);
    try {
      const saved = await api.saveRule(doc.name, draftRaw);
      setDoc(saved);
      setApplyWhen(saved.apply_when);
      setMode(saved.mode || "inline");
      setTrigger(saved.trigger ?? "");
      const parts = splitRaw(saved.raw_content);
      setFmRaw(parts.fm);
      setBody(parts.body);
      setDraftRaw(saved.raw_content);
      await onRulesChanged();
      if (andInstall) {
        await onInstallAll();
      } else {
        onToast("success", `Saved ${saved.name}`);
      }
    } catch (err) {
      onToast("error", `Failed to save ${doc.name}`, String(err));
    } finally {
      setSaving(false);
    }
  };

  const handleCreateRule = async () => {
    const trimmed = newRuleName.trim();
    if (!trimmed) return;
    try {
      const created = await api.createRule(trimmed);
      setCreating(false);
      setNewRuleName("");
      await onRulesChanged();
      onSelectRule(created.name);
      onToast("success", `Created ${created.name}`);
    } catch (err) {
      onToast("error", "Could not create rule", String(err));
    }
  };

  const handleDeleteRule = async () => {
    if (!doc || doc.is_init) return;
    if (!confirmDelete) {
      setConfirmDelete(true);
      return;
    }
    try {
      const deletedName = doc.name;
      await api.deleteRule(deletedName);
      setConfirmDelete(false);
      await onRulesChanged();
      onToast("info", `Deleted ${deletedName}`, "Run Compile & Install to prune ~/.brain.");
    } catch (err) {
      onToast("error", "Delete failed", String(err));
    }
  };

  const allTraitChips = useMemo(() => {
    const extras = applyWhen.filter(
      (t) => !(KNOWN_TRAITS as readonly string[]).includes(t)
    );
    return [...KNOWN_TRAITS, ...extras];
  }, [applyWhen]);

  return (
    <div className="view-split">
      <div className="split-sidebar">
        <div className="split-sidebar-header">
          <span style={{ fontWeight: 600, fontSize: 12.5 }}>
            Brain Files ({rules.length})
          </span>
          <button
            type="button"
            className="btn btn-sm"
            onClick={() => {
              setCreating((v) => !v);
              setNewRuleName("");
            }}
            title="Create new brain/*.md rule file"
          >
            <FilePlus size={13} />
            <span>New</span>
          </button>
        </div>

        {creating && (
          <div
            style={{
              padding: "8px 10px",
              borderBottom: "1px solid var(--border)",
              display: "flex",
              gap: 6,
            }}
          >
            <input
              type="text"
              className="input"
              style={{ flex: 1, minWidth: 0, height: 26, fontSize: 12 }}
              placeholder="rule-name.md"
              value={newRuleName}
              onChange={(e) => setNewRuleName(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === "Enter") handleCreateRule();
                if (e.key === "Escape") setCreating(false);
              }}
              autoFocus
            />
            <button
              type="button"
              className="btn btn-sm btn-primary"
              onClick={handleCreateRule}
            >
              Add
            </button>
          </div>
        )}

        <div className="split-sidebar-list">
          {rules.map((r) => {
            const selected = doc?.name === r.name;
            return (
              <button
                key={r.name}
                type="button"
                className={`list-card ${selected ? "active" : ""}`}
                onClick={() => onSelectRule(r.name)}
              >
                <div className="list-card-top">
                  <span className="list-card-name">{r.name}</span>
                  {r.is_init ? (
                    <span className="mini-tag accent">COMPILER</span>
                  ) : (
                    <span className="mini-tag">{r.mode}</span>
                  )}
                </div>
                <div className="list-card-sub">
                  <span>{r.line_count} lines</span>
                  {r.stubs.length > 0 && (
                    <span style={{ color: "#ffffff" }}>
                      • {r.stubs.length} stub{r.stubs.length > 1 ? "s" : ""}
                    </span>
                  )}
                </div>
              </button>
            );
          })}
        </div>
      </div>

      {doc ? (
        <div className="editor-pane">
          <div className="editor-header">
            <div style={{ minWidth: 0 }}>
              <div className="page-eyebrow">COMPILER DECK // RULE MATRIX</div>
              <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
                <span style={{ fontFamily: "var(--font-mono)", fontWeight: 700 }}>
                  brain/{doc.name}
                </span>
                {isDirty && <span className="status-badge drift">UNSAVED</span>}
                {doc.is_init && (
                  <span
                    className={`status-badge ${hasInitMarker ? "ok" : "error"}`}
                  >
                    {hasInitMarker ? (
                      <>
                        <CheckCircle2 size={11} />
                        <span>ANCHOR INTACT</span>
                      </>
                    ) : (
                      <>
                        <ShieldAlert size={11} />
                        <span>MISSING ## Non-Negotiable Rules</span>
                      </>
                    )}
                  </span>
                )}
              </div>
              <div
                className="selectable"
                style={{
                  fontSize: 11,
                  fontFamily: "var(--font-mono)",
                  color: "var(--text-muted)",
                  marginTop: 2,
                  overflow: "hidden",
                  textOverflow: "ellipsis",
                  whiteSpace: "nowrap",
                }}
              >
                {doc.path}
              </div>
            </div>

            <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
              {!doc.is_init && (
                <button
                  type="button"
                  className="btn btn-sm btn-danger"
                  onClick={handleDeleteRule}
                  onBlur={() => setConfirmDelete(false)}
                >
                  <Trash2 size={12} />
                  <span>{confirmDelete ? "Confirm Delete?" : "Delete"}</span>
                </button>
              )}

              <button
                type="button"
                className="btn btn-sm"
                onClick={() => handleSave(false)}
                disabled={!isDirty || saving || (doc.is_init && !hasInitMarker)}
              >
                <Save size={13} />
                <span>{saving ? "Saving..." : "Save"}</span>
              </button>

              <button
                type="button"
                className="btn btn-sm btn-primary"
                onClick={() => handleSave(true)}
                disabled={saving || (doc.is_init && !hasInitMarker)}
              >
                <Hammer size={13} />
                <span>Save &amp; Install Master</span>
              </button>
            </div>
          </div>

          {!doc.is_init && (
            <div className="frontmatter-panel">
              <div className="fm-row">
                <span className="fm-label">apply_when</span>
                <div className="chip-grid">
                  {allTraitChips.map((trait) => {
                    const selected = applyWhen.includes(trait);
                    return (
                      <button
                        key={trait}
                        type="button"
                        className={`trait-chip ${selected ? "selected" : ""}`}
                        aria-pressed={selected}
                        onClick={() => toggleTrait(trait)}
                      >
                        {trait}
                      </button>
                    );
                  })}
                </div>
              </div>

              <div className="fm-row">
                <span className="fm-label">mode</span>
                <div className="pill-group" role="tablist" aria-label="Rule mode">
                  {(["inline", "reference"] as const).map((m) => (
                    <button
                      key={m}
                      type="button"
                      role="tab"
                      aria-selected={mode === m}
                      className={`filter-pill ${mode === m ? "active" : ""}`}
                      onClick={() => updateFrontmatter(applyWhen, m, trigger)}
                    >
                      {m}
                    </button>
                  ))}
                </div>

                {mode === "reference" && (
                  <>
                    <span
                      className="fm-label"
                      style={{ width: "auto", minWidth: "unset", marginLeft: 10 }}
                    >
                      trigger
                    </span>
                    <input
                      type="text"
                      className="input selectable"
                      style={{ flex: 1, minWidth: 220, height: 28 }}
                      value={trigger}
                      placeholder="When should the agent read this playbook?"
                      onChange={(e) => updateFrontmatter(applyWhen, mode, e.target.value)}
                    />
                  </>
                )}
              </div>
            </div>
          )}

          {doc.is_init && !hasInitMarker && (
            <div style={{ padding: "12px 18px 0" }}>
              <div className="alert-strip error">
                <ShieldAlert size={15} style={{ flexShrink: 0 }} />
                <span>
                  <code>Sync-SkillFilesFromInit</code> slices <code>brain/init.md</code>{" "}
                  at the exact heading <code>## Non-Negotiable Rules</code>. Restore that
                  heading before saving.
                </span>
              </div>
            </div>
          )}

          {preview && preview.stubs.length > 0 && (
            <div style={{ padding: "12px 18px 0" }}>
              <div className="alert-strip warn">
                <AlertTriangle size={15} style={{ flexShrink: 0 }} />
                <span>
                  Detected {preview.stubs.length} unfilled stub placeholder
                  {preview.stubs.length > 1 ? "s" : ""}:{" "}
                  <code>{preview.stubs[0]}</code>
                </span>
              </div>
            </div>
          )}

          <div className="code-editor-wrap">
            <textarea
              className="code-textarea"
              value={doc.is_init ? draftRaw : body}
              onChange={(e) => {
                if (doc.is_init) {
                  setDraftRaw(e.target.value);
                } else {
                  updateBody(e.target.value);
                }
              }}
              spellCheck={false}
            />
          </div>

          <div className="editor-footer">
            <div style={{ display: "flex", alignItems: "center", gap: 14 }}>
              <span>
                Saved Hash:{" "}
                <code className="selectable" style={{ color: "var(--text-secondary)" }}>
                  {preview?.saved_fingerprint ?? "..."}
                </code>
              </span>
              <span>
                Draft Hash:{" "}
                <code
                  className="selectable"
                  style={{
                    color: preview?.changed
                      ? "var(--accent)"
                      : "var(--text-secondary)",
                  }}
                >
                  {preview?.draft_fingerprint ?? "..."}
                </code>
              </span>
              {preview?.changed && (
                <span className="mini-tag accent">FINGERPRINT WILL CHANGE</span>
              )}
            </div>
            <span>{draftRaw.split(/\r?\n/).length} total lines</span>
          </div>
        </div>
      ) : (
        <div className="view-scroll">
          <div className="empty-state">
            No brain rule files found. Configure your Brain repository path in Settings.
          </div>
        </div>
      )}
    </div>
  );
}
