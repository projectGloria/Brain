import { useCallback, useEffect, useRef, useState } from "react";
import { ArrowRight, Cpu, ShieldCheck, Zap } from "lucide-react";
import { MonjitaInteractive } from "./MonjitaInteractive";

interface SplashScreenProps {
  fingerprint: string;
  ruleCount: number;
  repoCount: number;
  onComplete: () => void;
}

const BOOT_LOGS = [
  "MOUNTING MASTER CORE  //  %USERPROFILE%\\.brain",
  "VERIFYING DETERMINISTIC SHA-256 FINGERPRINT",
  "LINKING CLAUDE CODE • CODEX • ANTIGRAVITY UPLINKS",
  "CYBERSPACE NEURAL INTERFACE ONLINE",
];

export function SplashScreen({
  fingerprint,
  ruleCount,
  repoCount,
  onComplete,
}: SplashScreenProps) {
  const [progress, setProgress] = useState(0);
  const [exiting, setExiting] = useState(false);
  const completedRef = useRef(false);

  const triggerExit = useCallback(() => {
    if (completedRef.current) return;
    completedRef.current = true;
    setExiting(true);
    window.setTimeout(() => {
      onComplete();
    }, 380);
  }, [onComplete]);

  useEffect(() => {
    const start = performance.now();
    const duration = 2050;
    let rafId = 0;
    let autoTimer = 0;

    const tick = (now: number) => {
      const elapsed = now - start;
      const ratio = Math.min(1, elapsed / duration);
      // Smooth ease-out curve
      const eased = 1 - Math.pow(1 - ratio, 2.2);
      setProgress(Math.round(eased * 100));
      if (ratio < 1) {
        rafId = window.requestAnimationFrame(tick);
      } else {
        autoTimer = window.setTimeout(() => {
          triggerExit();
        }, 280);
      }
    };

    rafId = window.requestAnimationFrame(tick);

    return () => {
      window.cancelAnimationFrame(rafId);
      window.clearTimeout(autoTimer);
    };
  }, [triggerExit]);

  useEffect(() => {
    const onKeyDown = (e: KeyboardEvent) => {
      if (e.key === "Enter" || e.key === " " || e.key === "Escape") {
        e.preventDefault();
        triggerExit();
      }
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [triggerExit]);

  const activeStepIndex = Math.min(
    BOOT_LOGS.length - 1,
    Math.floor((progress / 100) * BOOT_LOGS.length)
  );

  return (
    <div
      className={`splash-overlay ${exiting ? "exiting" : ""}`}
      role="dialog"
      aria-label="Monjita Cyberspace Boot Screen"
    >
      {/* Drag bar at top so splash can still move window */}
      <div className="splash-drag-strip" data-tauri-drag-region />

      {/* Decorative corner HUD telemetry */}
      <div className="splash-hud-corner top-left">
        <span className="hud-tag">SYS::NODE_01</span>
        <span className="hud-sub">PROJECT GLORIA // BRAIN OS v0.2.0</span>
      </div>
      <div className="splash-hud-corner top-right">
        <span className="hud-tag secondary">
          SHA256::{fingerprint === "............" ? "SYNCING..." : fingerprint}
        </span>
        <span className="hud-sub">
          {ruleCount} RULES • {repoCount} WORKSPACES
        </span>
      </div>

      <div className="splash-stage">
        {/* Left / Center Hologram Chamber */}
        <div className="splash-hologram-box">
          {/* Outer counter-rotating targeting rings */}
          <svg
            className="holo-ring outer"
            viewBox="0 0 360 360"
            aria-hidden="true"
          >
            <circle
              cx="180"
              cy="180"
              r="166"
              fill="none"
              stroke="rgba(255, 255, 255, 0.24)"
              strokeWidth="1.2"
              strokeDasharray="18 10 4 10"
            />
            <circle
              cx="180"
              cy="180"
              r="152"
              fill="none"
              stroke="rgba(255, 255, 255, 0.42)"
              strokeWidth="1.5"
              strokeDasharray="64 28 12 28"
            />
          </svg>

          <svg
            className="holo-ring inner"
            viewBox="0 0 360 360"
            aria-hidden="true"
          >
            <circle
              cx="180"
              cy="180"
              r="128"
              fill="none"
              stroke="rgba(255, 255, 255, 0.34)"
              strokeWidth="1.5"
              strokeDasharray="6 14"
            />
            <path
              d="M 180 18 L 180 34 M 180 326 L 180 342 M 18 180 L 34 180 M 326 180 L 342 180"
              stroke="#ffffff"
              strokeWidth="2.4"
              strokeLinecap="round"
            />
          </svg>

          {/* Pulsing energy core aligned behind Monjita's hands */}
          <div className="monjita-orb-aura" aria-hidden="true" />

          {/* Levitating Monjita artwork with hologram scan beam */}
          <div className="monjita-float-wrap">
            <MonjitaInteractive variant="splash" />
            <div className="holo-scan-beam" aria-hidden="true" />
          </div>

          <div className="holo-pedestal" aria-hidden="true" />
        </div>

        {/* Right Boot Telemetry & Enter CTA */}
        <div className="splash-telemetry-panel">
          <div className="splash-kicker">
            <Zap size={13} />
            <span>NEURAL RULE COMPILER INITIALIZED</span>
          </div>

          <h1 className="splash-title" data-text="MONJITA // BRAIN">
            MONJITA <span className="slash">//</span> BRAIN
          </h1>

          <p className="splash-desc">
            Cyberspace control deck for master rules, deterministic SHA-256
            fingerprints, and multi-tool agent skill uplinks.
          </p>

          <div className="boot-terminal">
            {BOOT_LOGS.map((line, idx) => {
              const done = idx < activeStepIndex || progress === 100;
              const active = idx === activeStepIndex && progress < 100;
              return (
                <div
                  key={line}
                  className={`boot-line ${done ? "done" : active ? "active" : "pending"}`}
                >
                  <span className="boot-prefix">
                    {done ? "[OK]" : active ? "[>>]" : "[..]"}
                  </span>
                  <span>{line}</span>
                </div>
              );
            })}
          </div>

          <div className="boot-progress-wrap">
            <div className="boot-progress-head">
              <span>SYNCHRONIZING MATRIX</span>
              <span>{progress}%</span>
            </div>
            <div className="boot-progress-track">
              <div
                className="boot-progress-fill"
                style={{ width: `${progress}%` }}
              />
            </div>
          </div>

          <div className="splash-cta-row">
            <button
              type="button"
              className="btn btn-primary btn-cyber-lg"
              onClick={triggerExit}
            >
              <Cpu size={15} />
              <span>ENTER CYBERSPACE</span>
              <ArrowRight size={15} />
            </button>

            <div className="splash-hint">
              <ShieldCheck size={13} />
              <span>Press Enter or Space to skip</span>
            </div>
          </div>
        </div>
      </div>
    </div>
  );
}
