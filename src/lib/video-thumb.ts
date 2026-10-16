import { convertFileSrc, invoke } from "@tauri-apps/api/core";
import { inTauri } from "./api";

/**
 * The first frame of a video, made by the system's own player: no decoder of ours.
 * A clip the system cannot play gives `null`, and its cell keeps the film icon.
 */
const LIMIT = 300;
const made = new Map<string, Promise<string | null>>();
let running = 0;
const waiting: Array<() => void> = [];

function turn(): Promise<void> {
  // One at a time: a grid full of clips must not make the processor busy.
  if (running < 1) {
    running++;
    return Promise.resolve();
  }
  return new Promise((go) => waiting.push(() => { running++; go(); }));
}
function done() {
  running--;
  waiting.shift()?.();
}

async function grab(path: string, width: number, wanted: () => boolean): Promise<string | null> {
  await turn();
  try {
    // Scrolled past while it waited: nothing to make.
    if (!wanted()) return null;
    const file = await invoke<string>("video_source", { path });
    const video = document.createElement("video");
    video.muted = true;
    video.preload = "auto";
    video.playsInline = true;
    video.crossOrigin = "anonymous";
    video.src = file.startsWith("http://127.0.0.1:") ? file : convertFileSrc(file);
    try {
      const ready = (event: string) => new Promise<void>((ok, no) => {
        const stop = window.setTimeout(() => no(new Error("timed out")), 8000);
        video.addEventListener(event, () => { window.clearTimeout(stop); ok(); }, { once: true });
        video.addEventListener("error", () => { window.clearTimeout(stop); no(new Error("cannot play")); }, { once: true });
      });
      const loaded = ready("loadeddata");
      video.load();
      await loaded;
      if (!video.videoWidth || !video.videoHeight) return null;
      const canvas = document.createElement("canvas");
      canvas.width = Math.min(width, video.videoWidth);
      canvas.height = Math.round((canvas.width * video.videoHeight) / video.videoWidth);
      canvas.getContext("2d")?.drawImage(video, 0, 0, canvas.width, canvas.height);
      return canvas.toDataURL("image/jpeg", 0.72);
    } finally {
      video.removeAttribute("src");
      video.load();
    }
  } catch {
    return null;
  } finally {
    done();
  }
}

/** The first frame of the video at `path`, `width` across, or `null` if the system cannot open it. `wanted` says whether anyone still needs it. */
export function videoThumb(path: string, width: number, wanted: () => boolean = () => true): Promise<string | null> {
  if (!inTauri) return Promise.resolve(null);
  // Sized in steps, so a grid being resized does not make a still for every width.
  const size = Math.min(1280, Math.max(160, Math.ceil(width / 160) * 160));
  const key = `${size}|${path}`;
  const have = made.get(key);
  if (have) return have;
  const next = grab(path, size, wanted).then((url) => {
    // Not made because nobody waited: forget it, so the next cell to ask can.
    if (url === null && !wanted()) made.delete(key);
    return url;
  });
  made.set(key, next);
  if (made.size > LIMIT) made.delete(made.keys().next().value as string);
  return next;
}
