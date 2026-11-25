import { useEffect, useMemo, useState } from "react";
import {
  AlertTriangle,
  BookOpen,
  Hammer,
  Lock,
  Save,
  ShieldAlert,
} from "lucide-react";
import { confirmDiscard, sameText, useUnsavedGuard } from "../hooks/useUnsavedGuard";
import type { SkillEntry } from "../types";

interface SkillManagerViewProps {
  skills: SkillEntry[];
  installing: boolean;
  legacySkillPresent: boolean;
  onInstallAll: () => void;
  onRemoveLegacy: () => void;
  onSaveSkill: (tool: string, skillName: string, content: string) => Promise<void>;
  onJumpToInitRule: () => void;
}

type ToolFilter = "all" | "claude-code" | "codex" | "antigravity";

export function SkillManagerView({
  skills,
  installing,
  legacySkillPresent,
  onInstallAll,
  onRemoveLegacy,
  onSaveSkill,
  onJumpToInitRule,
}: SkillManagerViewProps) {
  const [toolFilter, setToolFilter] = useState<ToolFilter>("all");
  const [selectedKey, setSelectedKey] = useState<string | null>(null);
  const [draft, setDraft] = useState("");
  const [saving, setSaving] = useState(false);

  const keyOf = (s: SkillEntry) => `${s.tool}::${s.skill_name}`;

  const filtered = useMemo(() => {
    if (toolFilter === "all") return skills;
    return skills.filter((s) => s.tool === toolFilter);
  }, [skills, toolFilter]);

  const activeSkill = useMemo(() => {
    if (!selectedKey) return filtered[0] ?? null;
    return filtered.find((s) => keyOf(s) === selectedKey) ?? filtered[0] ?? null;
  }, [filtered, selectedKey]);

  useEffect(() => {
    if (activeSkill) {
      setSelectedKey(keyOf(activeSkill));
      setDraft(activeSkill.content);
    } else {
      setDraft("");
    }
  }, [activeSkill?.tool, activeSkill?.skill_name, activeSkill?.content]);

  const isDirty = activeSkill ? !sameText(draft, activeSkill.content) : false;
  useUnsavedGuard(
    "skill-editor",
    isDirty,
    activeSkill ? `${activeSkill.tool}/${activeSkill.skill_name}` : "skill"
  );

  const selectSkill = (key: string) => {
    if (activeSkill && keyOf(activeSkill) !== key && !confirmDiscard("skill-editor")) return;
    setSelectedKey(key);
  };

  const statusTone = (status: string) => {
    if (status === "SYNCED") return "ok";
    if (status === "OUT_OF_SYNC") return "drift";
    return "muted";
  };

  const handleSave = async () => {
    if (!activeSkill || activeSkill.is_generated) return;
    setSaving(true);
    try {
      await onSaveSkill(activeSkill.tool, activeSkill.skill_name, draft);
    } finally {
      setSaving(false);
    }
  };

  return (
    <div style={{ display: "flex", flexDirection: "column", height: "100%", minHeight: 0 }}>
      <div
        style={{
          padding: "14px 20px",
          borderBottom: "1px solid var(--border)",
          background: "rgba(8, 8, 8, 0.84)",
          backdropFilter: "blur(12px)",
          display: "flex",
          alignItems: "center",
          justifyContent: "space-between",
          gap: 12,
          flexWrap: "wrap",
        }}
      >
        <div>
          <div className="page-eyebrow">AGENT UPLINKS // SKILL DISPATCH</div>
          <h1 className="page-title" style={{ fontSize: 16 }}>
            Multi-Tool Skill Manager
          </h1>
          <p className="page-subtitle">
            Inspect compiled and custom skills for Claude Code, Codex, and Antigravity.
          </p>
        </div>

        <div style={{ display: "flex", alignItems: "center", gap: 10 }}>
          <div className="pill-group" role="tablist" aria-label="Tool filter">
            {(
              [
                ["all", "All"],
                ["claude-code", "Claude Code"],
                ["codex", "Codex"],
                ["antigravity", "Antigravity"],
              ] as const
            ).map(([id, label]) => (
              <button
                key={id}
                type="button"
                className={`filter-pill ${toolFilter === id ? "active" : ""}`}
                role="tab"
                aria-selected={toolFilter === id}
                onClick={() => {
                  if (id !== toolFilter && !confirmDiscard("skill-editor")) return;
                  setToolFilter(id);
                }}
              >
                {label}
              </button>
            ))}
          </div>

          <button
            type="button"
            className="btn btn-primary"
            onClick={onInstallAll}
            disabled={installing}
          >
            <Hammer size={14} className={installing ? "spin" : ""} />
            <span>{installing ? "Installing..." : "Compile & Install All"}</span>
          </button>
        </div>
      </div>

      {legacySkillPresent && (
        <div
          className="alert-strip warn"
          style={{
            margin: "12px 20px 0",
            justifyContent: "space-between",
          }}
        >
          <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
            <AlertTriangle size={15} />
            <span>
              Legacy <code>agent-context-generator</code> skill found in Antigravity skills
              directory — may collide with <code>gloria-init-brain</code>.
            </span>
          </div>
          <button type="button" className="btn btn-sm" onClick={onRemoveLegacy}>
            Remove Legacy Skill
          </button>
        </div>
      )}

      <div className="view-split" style={{ marginTop: legacySkillPresent ? 10 : 0 }}>
        <div className="split-sidebar">
          <div className="split-sidebar-header">
            <span style={{ fontWeight: 600, fontSize: 12.5 }}>
              Skills ({filtered.length})
            </span>
          </div>

          <div className="split-sidebar-list">
            {filtered.map((skill) => {
              const key = keyOf(skill);
              const isSelected = activeSkill && keyOf(activeSkill) === key;
              return (
                <button
                  key={key}
                  type="button"
                  className={`list-card ${isSelected ? "active" : ""}`}
                  onClick={() => selectSkill(key)}
                >
                  <div className="list-card-top">
                    <span className="list-card-name">{skill.skill_name}</span>
                    <span
                      className={`status-badge ${statusTone(skill.status)}`}
                      style={{ fontSize: 10, padding: "1px 6px" }}
                    >
                      {skill.status.replace(/_/g, " ")}
                    </span>
                  </div>
                  <div className="list-card-sub">
                    <span>{skill.tool_label}</span>
                    {skill.is_generated && (
                      <span className="mini-tag accent">GENERATED</span>
                    )}
                  </div>
                </button>
              );
            })}
          </div>
        </div>

        {activeSkill ? (
          <div className="editor-pane">
            <div className="editor-header">
              <div style={{ minWidth: 0 }}>
                <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
                  <span style={{ fontFamily: "var(--font-mono)", fontWeight: 700 }}>
                    {activeSkill.tool_label} / {activeSkill.skill_name}
                  </span>
                  {activeSkill.is_generated && (
                    <span className="mini-tag accent">
                      <Lock size={10} style={{ marginRight: 4, verticalAlign: "middle" }} />
                      Compiled from brain/init.md
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
                  Installed target: {activeSkill.installed_path}
                </div>
              </div>

              <div style={{ display: "flex", gap: 8 }}>
                {activeSkill.is_generated ? (
                  <button
                    type="button"
                    className="btn btn-primary"
                    onClick={onJumpToInitRule}
                  >
                    <BookOpen size={14} />
                    <span>Edit brain/init.md in Rules</span>
                  </button>
                ) : (
                  <button
                    type="button"
                    className="btn btn-primary"
                    onClick={handleSave}
                    disabled={!isDirty || saving}
                  >
                    <Save size={14} />
                    <span>{saving ? "Saving..." : "Save SKILL.md"}</span>
                  </button>
                )}
              </div>
            </div>

            {activeSkill.is_generated && (
              <div style={{ padding: "12px 18px 0" }}>
                <div className="alert-strip info">
                  <ShieldAlert size={15} style={{ flexShrink: 0 }} />
                  <span>
                    Direct edits to <code>gloria-init-brain/SKILL.md</code> are
                    overwritten on compile. Edit <code>brain/init.md</code> and run
                    Compile &amp; Install.
                  </span>
                </div>
              </div>
            )}

            <div className="code-editor-wrap">
              <textarea
                className="code-textarea"
                value={draft}
                readOnly={activeSkill.is_generated}
                onChange={(e) => setDraft(e.target.value)}
                spellCheck={false}
              />
            </div>

            <div className="editor-footer">
              <span className="selectable" style={{ fontFamily: "var(--font-mono)" }}>
                Source: {activeSkill.repo_path}
              </span>
              <span>{draft.split(/\r?\n/).length} lines</span>
            </div>
          </div>
        ) : (
          <div className="view-scroll">
            <div className="empty-state">
              No skills found. Verify your Brain repository path in Settings.
            </div>
          </div>
        )}
      </div>
    </div>
  );
}
