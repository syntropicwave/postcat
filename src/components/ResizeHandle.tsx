import { useRef } from "react";

interface Props {
  /** "x" resizes width (drag left/right); "y" resizes height (drag up/down). */
  axis: "x" | "y";
  /** Called continuously during a drag with the pixel delta since the last event. */
  onDelta: (delta: number) => void;
  /** Double-click to reset to the default size. */
  onReset?: () => void;
  className?: string;
}

/**
 * A thin draggable divider. Uses pointer capture so the drag keeps tracking
 * even when the cursor moves over iframes/CodeMirror panes.
 */
export function ResizeHandle({ axis, onDelta, onReset, className }: Props) {
  const last = useRef(0);

  const onPointerDown = (e: React.PointerEvent) => {
    e.preventDefault();
    last.current = axis === "x" ? e.clientX : e.clientY;
    document.body.style.cursor = axis === "x" ? "col-resize" : "row-resize";
    document.body.style.userSelect = "none";

    const move = (ev: PointerEvent) => {
      const cur = axis === "x" ? ev.clientX : ev.clientY;
      onDelta(cur - last.current);
      last.current = cur;
    };
    const up = () => {
      window.removeEventListener("pointermove", move);
      window.removeEventListener("pointerup", up);
      document.body.style.cursor = "";
      document.body.style.userSelect = "";
    };
    window.addEventListener("pointermove", move);
    window.addEventListener("pointerup", up);
  };

  return (
    <div
      className={`resize-handle resize-${axis} ${className ?? ""}`}
      onPointerDown={onPointerDown}
      onDoubleClick={onReset}
      role="separator"
      aria-orientation={axis === "x" ? "vertical" : "horizontal"}
    />
  );
}
