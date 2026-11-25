import { useEffect, useRef } from "react";
import gloriaImg from "../assets/gloria.png";

interface GloriaInteractiveProps {
  className?: string;
  variant?: "sidebar" | "splash";
}

function clamp(value: number, min: number, max: number): number {
  return Math.max(min, Math.min(max, value));
}

export function GloriaInteractive({
  className = "",
  variant = "sidebar",
}: GloriaInteractiveProps) {
  const rigRef = useRef<HTMLDivElement | null>(null);
  const figureRef = useRef<HTMLImageElement | null>(null);

  useEffect(() => {
    const rigEl = rigRef.current;
    const figureEl = figureRef.current;
    if (!rigEl || !figureEl) return;

    const prefersReducedMotion = window.matchMedia(
      "(prefers-reduced-motion: reduce)"
    ).matches;
    if (prefersReducedMotion) return;

    const target = { nx: 0, ny: 0 };
    const current = { nx: 0, ny: 0 };
    let rafId = 0;

    const handleMouseMove = (e: MouseEvent) => {
      const rect = rigEl.getBoundingClientRect();
      if (rect.width === 0 || rect.height === 0) return;

      const centerX = rect.left + rect.width * 0.5;
      const centerY = rect.top + rect.height * 0.42;

      const scaleX = Math.max(window.innerWidth * 0.42, 260);
      const scaleY = Math.max(window.innerHeight * 0.42, 220);

      target.nx = clamp((e.clientX - centerX) / scaleX, -1, 1);
      target.ny = clamp((e.clientY - centerY) / scaleY, -1, 1);
    };

    const resetGaze = () => {
      target.nx = 0;
      target.ny = 0;
    };

    const tick = () => {
      current.nx += (target.nx - current.nx) * 0.12;
      current.ny += (target.ny - current.ny) * 0.12;

      const nx = current.nx;
      const ny = current.ny;

      const tx = nx * 2.5;
      const ty = ny * 1.8;
      const rotY = nx * 10;
      const rotX = -ny * 6;
      const rotZ = nx * 2.5;

      figureEl.style.transform = `perspective(700px) translate3d(${tx.toFixed(2)}px, ${ty.toFixed(2)}px, 0) rotateY(${rotY.toFixed(2)}deg) rotateX(${rotX.toFixed(2)}deg) rotateZ(${rotZ.toFixed(2)}deg)`;

      rafId = window.requestAnimationFrame(tick);
    };

    window.addEventListener("mousemove", handleMouseMove);
    window.addEventListener("mouseleave", resetGaze);
    window.addEventListener("blur", resetGaze);
    rafId = window.requestAnimationFrame(tick);

    return () => {
      window.cancelAnimationFrame(rafId);
      window.removeEventListener("mousemove", handleMouseMove);
      window.removeEventListener("mouseleave", resetGaze);
      window.removeEventListener("blur", resetGaze);
    };
  }, []);

  return (
    <div
      ref={rigRef}
      className={`gloria-rig ${variant === "splash" ? "splash-rig" : "sidebar-rig"} ${className}`}
    >
      <img
        ref={figureRef}
        src={gloriaImg}
        alt=""
        aria-hidden="true"
        className="gloria-layer gloria-figure-layer"
        draggable={false}
      />
    </div>
  );
}
