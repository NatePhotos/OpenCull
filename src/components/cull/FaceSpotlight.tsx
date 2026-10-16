import { faceOutline } from "@/lib/face-target";
import { useStore } from "@/lib/store";
import type { Photo } from "@/lib/types";

/** The face hovered in the face panel, picked out by dimming the rest of the photo. One SVG over the existing image. */
export function FaceSpotlight({ photo }: { photo: Photo }) {
  const enabled = useStore((s) => s.aiEnabled && s.faceSpotlight && s.showFaces && !s.settingsOpen);
  const faces = useStore((s) => s.ai[photo.index]?.faces);
  const hover = useStore((s) => s.hoveredFace);
  if (!enabled || !faces || hover?.photoIndex !== photo.index || hover.display !== photo.display || hover.version !== photo.version) return null;
  const active = faces.indexOf(hover.face);
  const corners = active >= 0 ? faceOutline(faces[active], photo) : null;
  if (!corners) return null;
  const hole = `M${corners.map((p) => `${p.x} ${p.y}`).join("L")}Z`;
  return <svg data-face-spotlight="" aria-hidden="true" className="pointer-events-none absolute inset-0 size-full overflow-hidden" viewBox="0 0 1 1" preserveAspectRatio="none">
    <path d={`M0 0H1V1H0Z ${hole}`} fill="black" fillOpacity={.72} fillRule="evenodd" />
    <path d={hole} fill="none" stroke="#111" strokeWidth="4" vectorEffect="non-scaling-stroke" />
    <path d={hole} fill="none" stroke="var(--color-brand, #59a6ff)" strokeWidth="2" vectorEffect="non-scaling-stroke" />
  </svg>;
}
