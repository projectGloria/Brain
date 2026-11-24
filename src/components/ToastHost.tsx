import { AlertTriangle, CheckCircle2, Info, XCircle } from "lucide-react";
import type { ToastItem } from "../types";

interface ToastHostProps {
  toasts: ToastItem[];
  onDismiss: (id: number) => void;
}

export function ToastHost({ toasts, onDismiss }: ToastHostProps) {
  if (toasts.length === 0) return null;

  return (
    <div className="toast-host" aria-live="polite">
      {toasts.map((t) => {
        const Icon =
          t.tone === "success"
            ? CheckCircle2
            : t.tone === "warning"
            ? AlertTriangle
            : t.tone === "error"
            ? XCircle
            : Info;

        const iconColor =
          t.tone === "warning" ? "#d4d4d8" : "#ffffff";

        return (
          <div
            key={t.id}
            className={`toast-card ${t.tone}`}
            onClick={() => onDismiss(t.id)}
            role="status"
          >
            <Icon size={16} style={{ color: iconColor, flexShrink: 0, marginTop: 1 }} />
            <div style={{ minWidth: 0 }}>
              <div className="toast-title">{t.title}</div>
              {t.detail && <div className="toast-detail">{t.detail}</div>}
            </div>
          </div>
        );
      })}
    </div>
  );
}
