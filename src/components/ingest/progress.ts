import { useRef, useState } from "react";
import type { IngestProgress } from "@/lib/api";

// How far a copy off a card has got, and how long it has left: a bar that
// only moves forward, a speed that one slow file does not swing about, and
// the words under the bar.

function rateText(bytesPerSecond: number): string {
  const mb = bytesPerSecond / 1_000_000;
  if (mb >= 1000) return `${(mb / 1000).toFixed(1)} GB/s`;
  if (mb >= 100) return `${mb.toFixed(0)} MB/s`;
  if (mb >= 1) return `${mb.toFixed(1)} MB/s`;
  return `${(bytesPerSecond / 1000).toFixed(0)} KB/s`;
}

function timeLeftText(seconds: number): string {
  if (seconds < 60) return `${Math.round(seconds)}s`;
  if (seconds < 90) return "about a minute";
  return `about ${Math.round(seconds / 60)} min`;
}

/** How long a burst of speed is remembered for, in seconds, and how often the speed is measured. */
const RATE_MEMORY = 3;
const RATE_SAMPLE = 0.4;

/** The progress of the copy running, if one is, kept and told as it reports. */
export function useCopyProgress() {
  const [progress, setProgress] = useState<IngestProgress | null>(null);
  const [fraction, setFraction] = useState(0);
  const [rate, setRate] = useState<number | null>(null);
  const [secondsLeft, setSecondsLeft] = useState<number | null>(null);
  const sample = useRef({ at: 0, bytes: 0, rate: null as number | null });

  /** A copy of `total` photos is starting. */
  const start = (total: number) => {
    setProgress({ done: 0, total, bytesCopied: 0, bytesSettled: 0, bytesTotal: 0 });
    setFraction(0);
    setRate(null);
    setSecondsLeft(null);
    sample.current = { at: performance.now() / 1000, bytes: 0, rate: null };
  };

  /** One report from the copy. */
  const report = (p: IngestProgress) => {
    setProgress(p);
    // By bytes, so a card of large files does not sit still and then
    // jump. Never backwards: reports from copies running side by side can
    // arrive a little out of order.
    const now = p.bytesTotal > 0 ? p.bytesSettled / p.bytesTotal : p.total > 0 ? p.done / p.total : 0;
    setFraction((was) => Math.min(1, Math.max(was, now)));

    const s = sample.current;
    const t = performance.now() / 1000;
    const elapsed = t - s.at;
    if (elapsed < RATE_SAMPLE) return;
    const measured = (p.bytesCopied - s.bytes) / elapsed;
    // Smoothed, so one slow file does not swing the estimate about.
    const weight = 1 - Math.exp(-elapsed / RATE_MEMORY);
    const smoothed = s.rate === null ? measured : s.rate + weight * (measured - s.rate);
    sample.current = { at: t, bytes: p.bytesCopied, rate: smoothed };
    setRate(smoothed);
    setSecondsLeft(smoothed > 0 ? Math.max(0, p.bytesTotal - p.bytesSettled) / smoothed : null);
  };

  /** The copy is over, or no longer shown. */
  const stop = () => setProgress(null);

  const status = (() => {
    if (!progress) return "";
    const parts = [`Copying ${progress.done}/${progress.total}`];
    if (rate !== null && rate > 0) parts.push(rateText(rate));
    if (secondsLeft !== null && Number.isFinite(secondsLeft) && secondsLeft >= 1) parts.push(`${timeLeftText(secondsLeft)} left`);
    return parts.length === 1 ? `${parts[0]}…` : parts.join(" · ");
  })();

  return { progress, fraction, status, start, report, stop };
}
