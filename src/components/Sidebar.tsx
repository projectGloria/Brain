import {
  AlertTriangle,
  BookOpen,
  Bot,
  Cpu,
  Dna,
  FileX2,
  LayoutDashboard,
  Settings,
  TerminalSquare,
} from "lucide-react";
import { GloriaInteractive } from "./GloriaInteractive";
import type { NavTab } from "../types";

interface SidebarProps {
  activeTab: NavTab;
  onSelectTab: (tab: NavTab) => void;
  driftCount: number;
  ruleCount: number;
  stubCount: number;
  cliAttentionCount: number;
  /** An install/update job is running on the CLI's tab. */
  cliBusy: boolean;
  runningAgents: number;
  gitignoreAttentionCount: number;
  dnaAttentionCount: number;
  skillsAttentionCount: number;
  masterExists: boolean;
  masterFingerprint: string;
  masterInSync: boolean;
  installing: boolean;
  legacySkillPresent: boolean;
  onRemoveLegacy: () => void;
}

export function Sidebar({
  activeTab,
  onSelectTab,
  driftCount,
  ruleCount,
  stubCount,
  cliAttentionCount,
  cliBusy,
  runningAgents,
  gitignoreAttentionCount,
  dnaAttentionCount,
  skillsAttentionCount,
  masterExists,
  masterFingerprint,
  masterInSync,
  installing,
  legacySkillPresent,
  onRemoveLegacy,
}: SidebarProps) {
  const navItems: Array<{
    id: NavTab;
    index: string;
    label: string;
    icon: typeof LayoutDashboard;
    badge?: string | number;
    warn?: boolean;
    busy?: boolean;
  }> = [
    {
      id: "dashboard",
      index: "01",
      label: "Dashboard",
      icon: LayoutDashboard,
      badge: driftCount > 0 ? driftCount : undefined,
      warn: driftCount > 0,
    },
    {
      id: "rules",
      index: "02",
      label: "Rules",
      icon: BookOpen,
      badge: ruleCount > 0 ? ruleCount : undefined,
    },
    {
      id: "skills",
      index: "03",
      label: "Skills",
      icon: Cpu,
      badge: skillsAttentionCount > 0 ? skillsAttentionCount : undefined,
      warn: skillsAttentionCount > 0,
    },
    {
      id: "clis",
      index: "04",
      label: "CLI's",
      icon: TerminalSquare,
      badge: cliAttentionCount > 0 ? cliAttentionCount : undefined,
      warn: cliAttentionCount > 0,
      busy: cliBusy,
    },
    {
      id: "runs",
      index: "05",
      label: "Agent Runs",
      icon: Bot,
      badge: runningAgents > 0 ? runningAgents : undefined,
      busy: runningAgents > 0,
    },
    {
      id: "gitignore",
      index: "06",
      label: ".gitignore",
      icon: FileX2,
      badge: gitignoreAttentionCount > 0 ? gitignoreAttentionCount : undefined,
      warn: gitignoreAttentionCount > 0,
    },
    {
      id: "dna",
      index: "07",
      label: "DNA",
      icon: Dna,
      badge: dnaAttentionCount > 0 ? dnaAttentionCount : undefined,
      warn: dnaAttentionCount > 0,
    },
    {
      id: "settings",
      index: "08",
      label: "Settings",
      icon: Settings,
    },
  ];

  return (
    <aside className="sidebar">
      <div>
        <div className="sidebar-section-label">CYBERSPACE // NAV</div>
        <nav className="nav-list" aria-label="Main navigation">
          {navItems.map((item) => {
            const Icon = item.icon;
            const isActive = activeTab === item.id;
            return (
              <button
                key={item.id}
                type="button"
                className={`nav-item ${isActive ? "active" : ""}`}
                onClick={() => onSelectTab(item.id)}
                aria-current={isActive ? "page" : undefined}
              >
                <span className="nav-item-left">
                  <span className="nav-index">{item.index}</span>
                  <Icon size={15} />
                  <span>{item.label}</span>
                  {item.busy && (
                    <span className="nav-busy" title="Working…" aria-label="Working" />
                  )}
                </span>
                {item.badge !== undefined && (
                  <span className={`nav-badge ${item.warn ? "warn" : ""}`}>
                    {item.badge}
                  </span>
                )}
              </button>
            );
          })}
        </nav>
      </div>

      {/* Prominent Full-Body Gloria Hologram Showcase */}
      <div
        className={`sidebar-gloria-stage ${installing ? "pulsing" : ""}`}
        aria-hidden="true"
      >
        <svg
          className="sidebar-gloria-reticle"
          viewBox="0 0 220 220"
        >
          <circle
            cx="110"
            cy="110"
            r="98"
            fill="none"
            stroke="rgba(255, 255, 255, 0.24)"
            strokeWidth="1"
            strokeDasharray="14 8 3 8"
          />
          <circle
            cx="110"
            cy="110"
            r="82"
            fill="none"
            stroke="rgba(255, 255, 255, 0.4)"
            strokeWidth="1.2"
            strokeDasharray="36 22 6 22"
          />
          <path
            d="M 110 6 L 110 18 M 110 202 L 110 214 M 6 110 L 18 110 M 202 110 L 214 110"
            stroke="rgba(255, 255, 255, 0.7)"
            strokeWidth="1.6"
            strokeLinecap="round"
          />
        </svg>

        <GloriaInteractive variant="sidebar" />

        <div className="sidebar-gloria-floor" />
      </div>

      <div className="sidebar-footer">
        {legacySkillPresent && (
          <button
            type="button"
            className="sidebar-warning-pill"
            onClick={onRemoveLegacy}
            title="Click to remove legacy agent-context-generator skill"
          >
            <AlertTriangle size={14} style={{ flexShrink: 0 }} />
            <span>Legacy skill detected — purge</span>
          </button>
        )}

        <div className="status-card">
          <div className="status-card-top">
            <span className="status-card-title">Gloria Core</span>
            <span
              className={`status-badge ${masterInSync ? "ok" : "drift"}`}
              style={{ fontSize: 9.5, padding: "1px 6px" }}
            >
              {installing
                ? "DEPLOYING"
                : masterInSync
                ? "SYNCED"
                : masterExists
                ? "STALE"
                : "UNINSTALLED"}
            </span>
          </div>

          <div className="status-kv">
            <span>Fingerprint</span>
            <code className="selectable">{masterFingerprint}</code>
          </div>
          <div className="status-kv">
            <span>Brain Matrix</span>
            <span>{ruleCount} files</span>
          </div>
          <div className="status-kv">
            <span>Stub Scan</span>
            <span style={{ color: stubCount > 0 ? "#ffffff" : "var(--text-secondary)" }}>
              {stubCount > 0 ? `${stubCount} incomplete` : "0 stubs"}
            </span>
          </div>
        </div>
      </div>
    </aside>
  );
}
