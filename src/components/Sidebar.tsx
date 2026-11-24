import {
  AlertTriangle,
  BookOpen,
  Cpu,
  LayoutDashboard,
  Settings,
} from "lucide-react";
import { MonjitaInteractive } from "./MonjitaInteractive";
import type { NavTab } from "../types";

interface SidebarProps {
  activeTab: NavTab;
  onSelectTab: (tab: NavTab) => void;
  driftCount: number;
  ruleCount: number;
  stubCount: number;
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
    },
    {
      id: "settings",
      index: "04",
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
              >
                <span className="nav-item-left">
                  <span className="nav-index">{item.index}</span>
                  <Icon size={15} />
                  <span>{item.label}</span>
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

      {/* Prominent Full-Body Monjita Hologram Showcase */}
      <div
        className={`sidebar-monjita-stage ${installing ? "pulsing" : ""}`}
        aria-hidden="true"
      >
        <svg
          className="sidebar-monjita-reticle"
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

        <MonjitaInteractive variant="sidebar" />

        <div className="sidebar-monjita-floor" />
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
            <span className="status-card-title">Monjita Core</span>
            <span
              className={`status-badge ${masterInSync ? "ok" : "drift"}`}
              style={{ fontSize: 9.5, padding: "1px 6px" }}
            >
              {installing
                ? "DEPLOYING"
                : masterInSync
                ? "SYNCED"
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
