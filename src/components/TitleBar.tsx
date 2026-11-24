import { useEffect, useState } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import {
  Copy,
  CopyCheck,
  Hammer,
  Minus,
  Sparkles,
  Square,
  X,
} from "lucide-react";
import monjitaIcon from "../assets/monjita.png";

interface TitleBarProps {
  fingerprint: string;
  masterInSync: boolean;
  installing: boolean;
  onInstallAll: () => void;
  onCopyFingerprint: () => void;
  onReplaySplash: () => void;
}

export function TitleBar({
  fingerprint,
  masterInSync,
  installing,
  onInstallAll,
  onCopyFingerprint,
  onReplaySplash,
}: TitleBarProps) {
  const [isMaximized, setIsMaximized] = useState(false);

  useEffect(() => {
    const win = getCurrentWindow();
    let mounted = true;

    const refresh = async () => {
      try {
        const max = await win.isMaximized();
        if (mounted) setIsMaximized(max);
      } catch {
        // ignore outside Tauri
      }
    };

    refresh();
    const unlistenPromise = win.onResized(() => {
      refresh();
    });

    return () => {
      mounted = false;
      unlistenPromise.then((u) => u()).catch(() => {});
    };
  }, []);

  const handleToggleMax = async () => {
    try {
      const win = getCurrentWindow();
      await win.toggleMaximize();
      setIsMaximized(await win.isMaximized());
    } catch {
      // ignore outside Tauri
    }
  };

  return (
    <header className="titlebar">
      <div
        className="titlebar-left"
        data-tauri-drag-region
        onDoubleClick={handleToggleMax}
      >
        <div className="titlebar-avatar-wrap" data-tauri-drag-region>
          <img src={monjitaIcon} alt="Monjita" className="titlebar-icon" />
        </div>
        <div className="titlebar-brand" data-tauri-drag-region>
          <span className="titlebar-title" data-tauri-drag-region>
            MONJITA <span className="titlebar-slash">//</span> BRAIN
          </span>
          <span className="titlebar-version" data-tauri-drag-region>
            v0.1.0
          </span>
        </div>
        <div className="telemetry-bars" aria-hidden="true" data-tauri-drag-region>
          <span />
          <span />
          <span />
          <span />
        </div>
      </div>

      <div
        className="titlebar-drag"
        data-tauri-drag-region
        onDoubleClick={handleToggleMax}
      />

      <div className="titlebar-actions">
        <button
          type="button"
          className="fp-pill"
          onClick={onCopyFingerprint}
          title="Copy <!-- brain-sync: ... --> marker"
        >
          <span className={`fp-dot ${masterInSync ? "" : "stale"}`} />
          <span>HASH::{fingerprint}</span>
          {masterInSync ? <Copy size={11} /> : <CopyCheck size={11} />}
        </button>

        <button
          type="button"
          className="btn btn-sm"
          onClick={onReplaySplash}
          title="Replay Monjita Holographic Boot Splash"
          aria-label="Replay Boot Splash"
        >
          <Sparkles size={13} style={{ color: "#ffffff" }} />
        </button>

        <button
          type="button"
          className="btn btn-sm btn-primary"
          onClick={onInstallAll}
          disabled={installing}
          title="Compile init.md, mirror ~/.brain, and install 3 tool skills"
        >
          <Hammer size={13} className={installing ? "spin" : ""} />
          <span>{installing ? "Deploying..." : "Compile & Deploy"}</span>
        </button>

        <div className="win-controls">
          <button
            type="button"
            className="win-btn"
            aria-label="Minimize"
            onClick={() => getCurrentWindow().minimize().catch(() => {})}
          >
            <Minus size={14} />
          </button>
          <button
            type="button"
            className="win-btn"
            aria-label={isMaximized ? "Restore" : "Maximize"}
            onClick={handleToggleMax}
          >
            <Square size={12} />
          </button>
          <button
            type="button"
            className="win-btn close"
            aria-label="Close"
            onClick={() => getCurrentWindow().close().catch(() => {})}
          >
            <X size={14} />
          </button>
        </div>
      </div>
    </header>
  );
}
