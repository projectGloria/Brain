import { useCallback, useRef, useState } from "react";
import { createPortal } from "react-dom";

interface HoverTipProps {
  text: string;
  children: React.ReactNode;
}

interface TipPos {
  x: number;
  y: number;
  below: boolean;
}

/**
 * Portal-rendered tooltip so explanations never clip inside `overflow: hidden` cards.
 */
export function HoverTip({ text, children }: HoverTipProps) {
  const anchorRef = useRef<HTMLSpanElement>(null);
  const [pos, setPos] = useState<TipPos | null>(null);

  const show = useCallback(() => {
    const el = anchorRef.current;
    if (!el || !text.trim()) return;
    const rect = el.getBoundingClientRect();
    const below = rect.top < 76;
    const x = Math.min(Math.max(rect.left + rect.width / 2, 150), window.innerWidth - 150);
    const y = below ? rect.bottom + 8 : rect.top - 8;
    setPos({ x, y, below });
  }, [text]);

  const hide = useCallback(() => {
    setPos(null);
  }, []);

  return (
    <>
      <span
        ref={anchorRef}
        className="hover-tip-anchor"
        onMouseEnter={show}
        onMouseLeave={hide}
        onFocus={show}
        onBlur={hide}
      >
        {children}
      </span>
      {pos &&
        createPortal(
          <div
            role="tooltip"
            className={`hover-tip-popup ${pos.below ? "below" : "above"}`}
            style={{ left: pos.x, top: pos.y }}
          >
            {text}
          </div>,
          document.body
        )}
    </>
  );
}
