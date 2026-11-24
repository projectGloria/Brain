import { useEffect, useRef } from "react";
import monjitaBodyImg from "../assets/monjita-body.png";
import monjitaHeadImg from "../assets/monjita-head.png";

interface MonjitaInteractiveProps {
  className?: string;
  variant?: "sidebar" | "splash";
}

function clamp(value: number, min: number, max: number): number {
  return Math.max(min, Math.min(max, value));
}

export function MonjitaInteractive({
  className = "",
  variant = "sidebar",
}: MonjitaInteractiveProps) {
  const rigRef = useRef<HTMLDivElement | null>(null);
  const headRef = useRef<HTMLImageElement | null>(null);
  const bodyRef = useRef<HTMLImageElement | null>(null);

  useEffect(() => {
    const rigEl = rigRef.current;
    const headEl = headRef.current;
    const bodyEl = bodyRef.current;
    if (!rigEl || !headEl || !bodyEl) return;

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

      // Anchor gaze center near Monjita's face/eyes (~53% X, ~36% Y)
      const headCenterX = rect.left + rect.width * 0.53;
      const headCenterY = rect.top + rect.height * 0.36;

      const scaleX = Math.max(window.innerWidth * 0.42, 260);
      const scaleY = Math.max(window.innerHeight * 0.42, 220);

      target.nx = clamp((e.clientX - headCenterX) / scaleX, -1, 1);
      target.ny = clamp((e.clientY - headCenterY) / scaleY, -1, 1);
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

      // Head rotates, tilts, and shifts cleanly around neck pivot
      const headTx = nx * 3.5;
      const headTy = ny * 2.5;
      const headRotY = nx * 14;
      const headRotX = -ny * 9;
      const headRotZ = nx * 4.5;

      headEl.style.transform = `perspective(700px) translate3d(${headTx.toFixed(2)}px, ${headTy.toFixed(2)}px, 0) rotateY(${headRotY.toFixed(2)}deg) rotateX(${headRotX.toFixed(2)}deg) rotateZ(${headRotZ.toFixed(2)}deg)`;

      // Body stays grounded with subtle counter-parallax
      const bodyTx = nx * 1.5;
      const bodyTy = ny * 1.1;
      const bodyRotY = nx * 4;
      const bodyRotX = -ny * 2;
      bodyEl.style.transform = `perspective(700px) translate3d(${bodyTx.toFixed(2)}px, ${bodyTy.toFixed(2)}px, 0) rotateY(${bodyRotY.toFixed(2)}deg) rotateX(${bodyRotX.toFixed(2)}deg)`;

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
      className={`monjita-rig ${variant === "splash" ? "splash-rig" : "sidebar-rig"} ${className}`}
    >
      <img
        ref={bodyRef}
        src={monjitaBodyImg}
        alt="Monjita body"
        className="monjita-layer monjita-body-layer"
        draggable={false}
      />
      <img
        ref={headRef}
        src={monjitaHeadImg}
        alt="Monjita"
        className="monjita-layer monjita-head-layer"
        draggable={false}
      />
    </div>
  );
}
