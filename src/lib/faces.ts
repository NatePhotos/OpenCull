// Runs face analysis over the open folder, in the background.

import { useEffect, useRef } from "react";

import { api, logged } from "./api";
import { useStore } from "./store";

/** How many photos either side of the one on screen the scan is asked to do first. */
const LOOK_REACH = 16;
/**
 * How long a photo is rested on before the scan is pointed at it. A held
 * arrow key passes thirty photos a second, and none of those is being
 * looked at.
 */
const LOOK_SETTLE_MS = 120;

/**
 * Keeps the open folder analysed.
 *
 * Starts once the folder's details are in — bursts are not known before
 * that, and a rank needs its burst. Works outward from the photo on screen,
 * so the frames about to be looked at are the first to have their faces.
 * Stepping through photos restarts nothing; a different folder, or the
 * switch being turned back on, does. Moving somewhere the scan has not
 * reached points it there: that photo and the ones round it are done next,
 * and it then carries on where it was.
 */
export function useFaceScan(): void {
  // A self-test decides for itself what is analysed and when.
  const enabled = useStore((s) => s.aiEnabled && !s.selfTest);
  const session = useStore((s) => (s.session !== null && s.metaProgress >= 1 ? s.session.id : null));
  const pause = useStore((s) => s.aiPause);
  const rescan = useStore((s) => s.aiRescan);

  useEffect(() => void api.aiSetYield(pause).catch(logged("pausing analysis")), [pause]);

  // What counts as a group has changed: compare what is already analysed
  // again, under the new rule. Nothing is analysed again for this.
  const smart = useStore((s) => s.smartGroups);
  const was = useRef(smart);
  useEffect(() => {
    if (was.current === smart) return;
    was.current = smart;
    useStore.getState().regroup();
  }, [smart]);

  // Listened for rather than read as the rest are: this runs on every photo
  // stepped to, and must not redraw the workspace each time.
  useEffect(() => {
    if (!enabled || session === null) return;
    let timer: number | undefined;
    let last = useStore.getState().current;
    const look = () => {
      const s = useStore.getState();
      if (s.session?.id !== session || !s.aiRunning || s.current < 0) return;
      const at = s.view.indexOf(s.current);
      const near = [s.current];
      for (let d = 1; at >= 0 && d <= LOOK_REACH; d++) {
        if (at + d < s.view.length) near.push(s.view[at + d]);
        if (at - d >= 0) near.push(s.view[at - d]);
      }
      void api.aiLookAt(near.filter((index) => s.ai[index] === undefined)).catch(logged("analysing the photo on screen first"));
    };
    const stop = useStore.subscribe((s) => {
      if (s.current === last) return;
      last = s.current;
      window.clearTimeout(timer);
      if (s.aiRunning) timer = window.setTimeout(look, LOOK_SETTLE_MS);
    });
    return () => {
      stop();
      window.clearTimeout(timer);
    };
  }, [enabled, session]);

  useEffect(() => {
    if (!enabled || session === null) return;
    const { order, current, photos } = useStore.getState();
    if (photos.length === 0) return;
    const here = Math.max(0, order.indexOf(current));
    const nearest = order.map((index, place) => ({ index, far: Math.abs(place - here) })).sort((a, b) => a.far - b.far).map((o) => o.index);

    useStore.setState({ aiRunning: true });
    void api
      .aiScan(session, nearest, (event) => {
        const s = useStore.getState();
        if (s.session?.id !== session) return; // a folder since closed
        if (event.type === "batch") s.addAnalysis(event.items);
        else s.set("aiRunning", false);
      })
      .catch(() => useStore.getState().session?.id === session && useStore.setState({ aiRunning: false }));

    return () => {
      void api.aiStop().catch(logged("stopping analysis"));
      if (useStore.getState().session?.id === session) useStore.setState({ aiRunning: false });
    };
  }, [enabled, session, rescan]);
}
