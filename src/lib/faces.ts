import { isVideo } from "./media";
// Runs face analysis over the open folder, in the background.

import { useEffect, useRef } from "react";

import { api } from "./api";
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
  const advanced = useStore((s) => s.aiAdvanced);

  useEffect(() => void api.aiSetYield(pause).catch(() => {}), [pause]);

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
      void api.aiLookAt(near.filter((index) => s.ai[index] === undefined && !isVideo(s.photos[index].path))).catch(() => {});
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

  // Judged by other rules, nothing found so far stands: it is put away and
  // the folder gone through again. What the other rules found is kept in
  // its own memory, so switching back costs nothing.
  const judged = useRef(advanced);
  useEffect(() => {
    if (judged.current === advanced) return;
    judged.current = advanced;
    useStore.setState({ ai: [], compared: [], ranks: [], poses: [], aiDone: 0 });
  }, [advanced]);

  useEffect(() => {
    if (!enabled || session === null) return;
    const { order, current, photos } = useStore.getState();
    if (photos.length === 0) return;
    const here = Math.max(0, order.indexOf(current));
    const nearest = order.map((index, place) => ({ index, far: Math.abs(place - here) })).sort((a, b) => a.far - b.far).map((o) => o.index).filter((i) => !isVideo(photos[i].path));

    useStore.setState({ aiRunning: true });
    // A scan that has been stopped may still have answers on their way.
    // Judged by other rules, they must not be taken for this scan's.
    let live = true;
    void api
      .aiScan(session, nearest, advanced, (event) => {
        const s = useStore.getState();
        if (!live || s.session?.id !== session) return; // stopped, or a folder since closed
        if (event.type === "batch") s.addAnalysis(event.items);
        else s.set("aiRunning", false);
      })
      .catch(() => useStore.getState().session?.id === session && useStore.setState({ aiRunning: false }));

    return () => {
      live = false;
      void api.aiStop().catch(() => {});
      if (useStore.getState().session?.id === session) useStore.setState({ aiRunning: false });
    };
  }, [enabled, session, advanced]);
}
