import { useEffect, useState } from "react";
import { api, type Volume } from "./api";

/** How often attached cards and drives are looked for. */
const LOOK_EVERY_MS = 2500;

/**
 * The cards and drives attached, kept up to date while `active`. `null`
 * until the first look has come back.
 *
 * One look is shared by the home screen, which offers a card as it is
 * plugged in, and the ingest view, which lists them.
 */
export function useVolumes(active: boolean): Volume[] | null {
  const [volumes, setVolumes] = useState<Volume[] | null>(null);
  useEffect(() => {
    if (!active) return;
    let stale = false;
    // A look that fails is not reported: it is tried again in a moment, and
    // a drive that does not answer would otherwise fill the log.
    const look = () => void api.ingestVolumes().then((found) => !stale && setVolumes(found), () => {});
    look();
    const timer = setInterval(look, LOOK_EVERY_MS);
    return () => {
      stale = true;
      clearInterval(timer);
    };
  }, [active]);
  return volumes;
}
