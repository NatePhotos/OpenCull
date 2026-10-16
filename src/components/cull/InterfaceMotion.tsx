import { useEffect, useLayoutEffect, useRef, useState, type CSSProperties, type ReactNode } from "react";
import { useStore } from "@/lib/store";

export function useInterfaceMotion() {
  const enabled = useStore(s => s.experimentalTransitions);
  const [reduced, setReduced] = useState(() => matchMedia("(prefers-reduced-motion: reduce)").matches);
  useEffect(() => {
    const query = matchMedia("(prefers-reduced-motion: reduce)");
    const update = () => setReduced(query.matches);
    query.addEventListener("change", update);
    return () => query.removeEventListener("change", update);
  }, []);
  return enabled && !reduced;
}

/** Retain exiting content only until its size transition ends. Never persist visibility. */
export function MotionRegion({ open, width, children, className = "" }: { open: boolean; width?: string; children: ReactNode; className?: string }) {
  const motion = useInterfaceMotion();
  const resizing = useStore(s => s.panelResizing);
  const [mounted, setMounted] = useState(open);
  const [expanded, setExpanded] = useState(open);
  useLayoutEffect(() => {
    if (!motion) { setMounted(open); setExpanded(open); return; }
    if (open) {
      setMounted(true);
      const frame = requestAnimationFrame(() => setExpanded(true));
      return () => cancelAnimationFrame(frame);
    }
    setExpanded(false);
    const timer = setTimeout(() => setMounted(false), 180);
    return () => clearTimeout(timer);
  }, [open, motion]);
  if (!open && (!motion || !mounted)) return null;
  const shown = motion ? expanded : open;
  const style: CSSProperties = width
    ? { width: shown ? width : 0, transition: motion && !resizing ? "width 160ms ease-out" : undefined }
    : { display: "grid", gridTemplateRows: shown ? "1fr" : "0fr", transition: motion ? "grid-template-rows 160ms ease-out" : undefined };
  return <div data-motion-region="" inert={!open} aria-hidden={!open} className={`min-h-0 shrink-0 overflow-hidden ${className}`} style={style}>
    <div className="h-full min-h-0 overflow-hidden">{children}</div>
  </div>;
}

/** Keep the existing image layers and zoom lifecycle; animate only the surrounding view. */
export function PhotoTransition({ identity, children }: { identity: string; children: ReactNode }) {
  const motion = useInterfaceMotion();
  const ref = useRef<HTMLDivElement>(null);
  useEffect(() => {
    if (!motion) return;
    const animation = ref.current?.animate([{ opacity: .82 }, { opacity: 1 }], { duration: 160, easing: "ease-out" });
    return () => animation?.cancel();
  }, [identity, motion]);
  return <div ref={ref} className="size-full">{children}</div>;
}
