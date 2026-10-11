import { useEffect, useState } from "react";
import { api, type Volume } from "./api";

/** How often attached cards and drives are looked for. */
const LOOK_EVERY_MS = 2500;

/**
 * The cards and drives attached, polled while `active`; `null` until the
 * first answer. Used by the home screen and the ingest view.
 */
export function useVolumes(active: boolean): Volume[] | null {
  const [volumes, setVolumes] = useState<Volume[] | null>(null);
  useEffect(() => {
    if (!active) return;
    let stale = false;
    // Failures are not logged: the next poll retries, and an unresponsive
    // drive would otherwise fill the log.
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
