import {
  AlertTriangle,
  BookOpen,
  Cpu,
  LayoutDashboard,
  Settings,
} from "lucide-react";
import type { NavTab } from "../types";

interface SidebarProps {
  activeTab: NavTab;
  onSelectTab: (tab: NavTab) => void;
  driftCount: number;
  ruleCount: number;
  stubCount: number;
  masterFingerprint: string;
  masterInSync: boolean;
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
  legacySkillPresent,
  onRemoveLegacy,
}: SidebarProps) {
  const navItems: Array<{
    id: NavTab;
    label: string;
    icon: typeof LayoutDashboard;
    badge?: string | number;
    warn?: boolean;
  }> = [
    {
      id: "dashboard",
      label: "Dashboard",
      icon: LayoutDashboard,
      badge: driftCount > 0 ? driftCount : undefined,
      warn: driftCount > 0,
    },
    {
      id: "rules",
      label: "Rules",
      icon: BookOpen,
      badge: ruleCount > 0 ? ruleCount : undefined,
    },
    {
      id: "skills",
      label: "Skills",
      icon: Cpu,
    },
    {
      id: "settings",
      label: "Settings",
      icon: Settings,
    },
  ];

  return (
    <aside className="sidebar">
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
                <Icon size={16} />
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

      <div className="sidebar-footer">
        {legacySkillPresent && (
          <button
            type="button"
            className="sidebar-warning-pill"
            onClick={onRemoveLegacy}
            title="Click to remove legacy agent-context-generator skill"
          >
            <AlertTriangle size={14} style={{ flexShrink: 0 }} />
            <span>Legacy skill detected — click to clean</span>
          </button>
        )}

        <div className="status-card">
          <div className="status-card-header">
            <span>Master Brain</span>
            <span
              className={`status-badge ${masterInSync ? "ok" : "drift"}`}
              style={{ fontSize: 10, padding: "1px 6px" }}
            >
              {masterInSync ? "SYNCED" : "UNINSTALLED"}
            </span>
          </div>

          <div className="status-kv">
            <span>Fingerprint</span>
            <code className="selectable">{masterFingerprint}</code>
          </div>
          <div className="status-kv">
            <span>Rules</span>
            <span>{ruleCount} files</span>
          </div>
          <div className="status-kv">
            <span>Stubs</span>
            <span style={{ color: stubCount > 0 ? "var(--amber)" : "var(--green)" }}>
              {stubCount > 0 ? `${stubCount} incomplete` : "0 stubs"}
            </span>
          </div>
        </div>
      </div>
    </aside>
  );
}
