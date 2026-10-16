// A stand-in backend for working on the layout in a browser tab. Development
// only: `api.ts` never loads this in a built app.

import type { Backend } from "./api";
import type { Exif, Meta, Scanned } from "./types";

const COUNT = 240;
let activeImages: Scanned[];

const images: Scanned[] = Array.from({ length: COUNT }, (_, i) => {
  const name = `A7V${String(2751 + i).padStart(5, "0")}.ARW`;
  return { path: `/Photos/Sample Shoot/${name}`, display: `/Photos/Sample Shoot/${name}`, name, subpath: null, version: "1", size: 24_000_000 + i * 1000, modified: 0 };
});

function exif(i: number, ms: number): Exif {
  const d = new Date(ms);
  const pad = (n: number) => String(n).padStart(2, "0");
  return {
    make: "SONY",
    model: "ILCE-7M5",
    lens: "FE 85mm F1.4 GM II",
    focalLength: 85,
    focalLength35mm: 85,
    aperture: [1.4, 2, 2.8, 4][i % 4],
    shutter: [1 / 250, 1 / 500, 1 / 1000, 1 / 60][i % 4],
    iso: [100, 400, 1600, 6400][i % 4],
    exposureComp: i % 5 === 0 ? 0.3 : 0,
    date: `${d.getUTCFullYear()}:${pad(d.getUTCMonth() + 1)}:${pad(d.getUTCDate())} ${pad(d.getUTCHours())}:${pad(d.getUTCMinutes())}:${pad(d.getUTCSeconds())}`,
    captureMs: ms,
    width: 7008,
    height: 4672,
    latitude: null,
    longitude: null,
  };
}

/** A flat picture with the frame's number on it, portrait every seventh. */
function picture(i: number, long: number): string {
  const portrait = i % 7 === 3;
  const [w, h] = portrait ? [Math.round(long / 1.5), long] : [long, Math.round(long / 1.5)];
  const hue = (i * 37) % 360;
  const svg = `<svg xmlns="http://www.w3.org/2000/svg" width="${w}" height="${h}" viewBox="0 0 ${w} ${h}">
    <defs><linearGradient id="g" x1="0" y1="0" x2="1" y2="1">
      <stop offset="0" stop-color="hsl(${hue} 45% 42%)"/><stop offset="1" stop-color="hsl(${(hue + 50) % 360} 55% 18%)"/>
    </linearGradient></defs>
    <rect width="100%" height="100%" fill="url(#g)"/>
    <circle cx="${w * 0.5}" cy="${h * 0.42}" r="${Math.min(w, h) * 0.16}" fill="hsl(${hue} 30% 78% / 0.55)"/>
    <text x="50%" y="88%" fill="rgba(255,255,255,.75)" font-family="sans-serif" font-size="${long / 14}" text-anchor="middle">${i + 1}</text>
  </svg>`;
  return `data:image/svg+xml,${encodeURIComponent(svg)}`;
}

const later = <T,>(value: T, ms = 20) => new Promise<T>((r) => setTimeout(() => r(value), ms));

export const mock: Backend = {
  appInfo: () => later({ platform: "macos", version: "dev", cacheDir: "/tmp/opencull", openPath: null, selfTest: false }),
  // Untimed, so the slower path — filename order first, shooting order once
  // the metadata is in — is the one a browser tab exercises.
  openFolder: (_path, _include, _kinds, members) => { activeImages = members ?? images; return later({ session: 1, images: activeImages, subfolderCount: 2, leftOut: 0, times: activeImages.map(() => null), timed: false, groups: null }, 150); },
  closeFolder: () => later(undefined),
  loadMetadata: async (_session, onEvent) => {
    // Bursts of one to six frames, a few seconds apart.
    let ms = Date.UTC(2026, 7, 1, 14, 0, 0);
    const groups: (number | null)[] = [];
    const items: Meta[] = [];
    let i = 0;
    let group = 0;
    while (i < activeImages.length) {
      const run = 1 + ((i * 7) % 6);
      for (let k = 0; k < run && i < activeImages.length; k++, i++) {
        ms += k === 0 ? 4000 + (i % 5) * 3000 : 120;
        const portrait = i % 7 === 3;
        items.push({
          index: i,
          marks: { rating: i % 11 === 0 ? 5 : i % 13 === 0 ? 3 : 0, flag: i % 17 === 0 ? "reject" : i % 19 === 0 ? "pick" : "none", color: i % 23 === 0 ? "green" : "none" },
          exif: exif(i, ms),
          fullWidth: portrait ? 4672 : 7008,
          fullHeight: portrait ? 7008 : 4672,
          // A few framed already, to show what that looks like: a tight
          // crop, a straightened one, one lying on its side, and one cropped
          // and mirrored.
          framing:
            i === 1
              ? { turn: 1, crop: { cx: 4200, cy: 2000, w: 3000, h: 2000, angle: 0 } }
              : i === 2
                ? { turn: 1, crop: { cx: 3504, cy: 2336, w: 5400, h: 3600, angle: 6 } }
                : i === 4
                  ? { turn: 6, crop: null }
                  : i === 5
                    ? { turn: 2, crop: { cx: 2600, cy: 2336, w: 3200, h: 4000, angle: -4 } }
                    : { turn: 1, crop: null },
          framable: i % 29 !== 28,
        });
        groups.push(run > 1 ? group : null);
      }
      if (run > 1) group++;
    }
    for (let at = 0; at < items.length; at += 64) {
      await later(null, 60);
      onEvent({ type: "batch", items: items.slice(at, at + 64) });
    }
    onEvent({ type: "done", groups });
  },
  prefetch: () => later(undefined),
  warm: (items) => later(items.length),
  warmRemaining: () => later(0),
  setMarks: () => later(undefined),
  flushMarks: () => later(undefined),
  setFraming: () => later(undefined),
  moveToTrash: () => later([]),
  transferPhotos: (items, _destination, moving) => later({ placed: items.map((i) => i.path), moved: moving ? items.map((i) => i.path) : [], skipped: 0, failed: [], sidecarFailed: [] }),
  shipConflicts: (items) => later(items.length > 3 ? 2 : 0),
  ship: async (items, destination, moving, _skip, onProgress) => {
    const paths = items.map((i) => i.path);
    for (let done = 1; done <= paths.length; done++) {
      await later(null, 40);
      onProgress(done);
    }
    return { placed: paths, moved: destination && moving ? paths : [], skipped: 0, failed: [], sidecarFailed: [] };
  },
  editingApps: () =>
    later([
      { id: "lightroomClassic", name: "Lightroom Classic", installed: true, takesCatalog: true },
      { id: "lightroom", name: "Lightroom", installed: false, takesCatalog: false },
      { id: "aftershoot", name: "Aftershoot", installed: true, takesCatalog: false },
      { id: "fileBrowser", name: "Reveal in Finder", installed: true, takesCatalog: false },
    ]),
  lightroomCatalogs: () => later(["/Users/sample/Pictures/Lightroom/Weddings 2026.lrcat", "/Users/sample/Pictures/Lightroom/Personal.lrcat"], 600),
  openInEditor: () => later(undefined),
  pathsExist: (paths) => later(paths.map(() => true)),
  aiScan: async (_session, order, advanced, onEvent) => {
    onEvent({ type: "cache", cached: 0, pending: order.length });
    for (let at = 0; at < order.length; at += 12) {
      await later(null, 120);
      onEvent({
        type: "batch",
        items: order.slice(at, at + 12).map((index) => {
          const n = index % 5;
          const faces = Array.from({ length: n }, (_, f) => ({
            x: 0.12 + f * 0.17,
            y: 0.28 + (f % 2) * 0.06,
            w: f === 3 ? 0.04 : 0.11,
            h: f === 3 ? 0.06 : 0.165,
            subject: f !== 3,
            reason: f === 3 ? "Background — small beside the others" : null,
            eyesShut: index % 7 === 0 && f === 0,
            lookingAway: index % 4 === 1 && f === 1,
            maybeAway: advanced && index % 5 === 3 && f === 2,
            sharpness: 35 + ((index * 13 + f * 29) % 60),
            // Now and then someone is caught half way through a blink.
            eye: (index + f) % 6 === 2 ? 0.15 : 0.28 + ((index + f) % 3) * 0.01,
            eyesRead: f !== 3,
            // A spread of the states a face can be in.
            state: f === 3 || !advanced ? null : ["Smiling · Eyes Open", "Looking Down", "Eyes Fully Open", "Laughing · Eyes Closed", "Eyes Barely Open", "Facing Away", "Eyes Closed"][(index + f * 3) % 7],
            concern: f === 3 || !advanced ? 0 : [0, 0, 0, 0, 2, 0, 3][(index + f * 3) % 7],
            excused: f === 3 || !advanced ? 0 : [0, 0.85, 0, 0.85, 0, 0.85, 0][(index + f * 3) % 7],
          }));
          // Nobody in it: what it is about instead, somewhere off centre.
          const key = n === 0 ? { x: 0.2 + (index % 3) * 0.1, y: 0.15 + (index % 2) * 0.2, w: 0.4, h: 0.4 } : null;
          return { index, faces, score: 20 + ((index * 37) % 75), focus: n === 0 ? null : 0.8, gaze: n === 0 ? null : 1, key };
        }),
      });
    }
    onEvent({ type: "done", finished: true });
  },
  // A stand-in for the real thing: a pose is a burst, and the same face is
  // the same place in the list.
  aiCompare: (runs) =>
    later(
      runs.map((run) =>
        run.map((frame) => {
          const frames = run.filter((other) => other.burst !== null && other.burst === frame.burst);
          const eyes = frame.faces.map((face, f) => {
            const best = Math.max(...frames.map((other) => (other.faces[f]?.eyesRead ? other.faces[f].eye : 0)));
            return face.eyesRead && best > 0 ? Math.min(1, face.eye / best) : null;
          });
          const worst = Math.min(1, ...eyes.filter((e): e is number => e !== null));
          return { pose: frame.burst ?? -1 - run.indexOf(frame), score: frame.score === null ? null : Math.round(frame.score * (0.4 + 0.6 * worst)), eyes };
        }),
      ),
    ),
  aiAccelerationInfo: async () => null,
  mobileClipStatus: async () => ({ ready: false, bytes: 45_543_630, model: "MobileCLIP-S0", backend: "CPU" as const }),
  mobileClipInstall: async () => { throw new Error("Model downloads are available in the desktop app."); },
  aiLookAt: () => later(undefined),
  aiStop: () => later(undefined),
  aiSetYield: () => later(undefined),
  aiCacheUsage: () => later([2_400_000, 14] as [number, number]),
  aiClearCache: () => later(undefined),
  // `?update` in the address shows what a newer version being found looks like.
  updateCheck: () => later(new URLSearchParams(location.search).has("update") ? { version: "9.9.9", current: "0.0.0" } : null, 300),
  updateInstall: async (onProgress) => {
    for (let done = 0; done <= 20; done++) {
      await later(null, 80);
      onProgress(done, 20);
    }
    throw new Error("this is the preview, so nothing was installed");
  },
  openReleasePage: () => later(undefined),
  ingestVolumes: () =>
    later([
      { path: "/Volumes/SONY_CARD", name: "SONY_CARD", removable: true, cameraCard: true },
      { path: "/Volumes/Archive SSD", name: "Archive SSD", removable: true, cameraCard: false },
    ]),
  ingestBrowse: (path) =>
    later(
      path.endsWith("DCIM")
        ? { folders: [{ path: `${path}/100CANON`, name: "100CANON", imageCount: images.length }], imageCount: 0 }
        : path.endsWith("100CANON") || path.endsWith("Weddings")
          ? { folders: [], imageCount: images.length }
          : { folders: [{ path: `${path}/DCIM`, name: "DCIM", imageCount: 0 }, { path: `${path}/Weddings`, name: "Weddings", imageCount: 0 }], imageCount: 0 },
      200,
    ),
  ingestSources: (_sources) => later(images.slice(0, 60)),
  ingestScan: () => later(images, 300),
  ingestDetails: async (paths, onFound) => {
    for (let at = 0; at < paths.length; at += 64) {
      await later(null, 80);
      onFound(paths.slice(at, at + 64).map((_, i) => ({ index: at + i, captureMs: 1_790_000_000_000 + (at + i) * 4000, rating: (at + i) % 9 === 0 ? 3 : 0 })));
    }
  },
  ingestConflicts: (items) => later(items.length > 5 ? 2 : 0),
  ingestCopy: async (items, _destination, _policy, onProgress) => {
    const each = 25_000_000;
    for (let done = 0; done <= items.length; done++) {
      await later(null, 90);
      onProgress({ done, total: items.length, bytesCopied: done * each, bytesSettled: done * each, bytesTotal: items.length * each });
    }
    return { status: "success", message: null };
  },
  ingestCancel: () => later(undefined),
  onQuitRefused: () => () => {},
  onJpegNotWritten: () => () => {},
  onMarksNotSaved: () => () => {},
  onFramingNotSaved: () => () => {},
  histogram: (path) => {
    const seed = path.length;
    const bell = (centre: number) => Array.from({ length: 256 }, (_, x) => Math.round(900 * Math.exp(-((x - centre) ** 2) / 2600)));
    return later({ r: bell(90 + (seed % 60)), g: bell(110 + (seed % 40)), b: bell(140 - (seed % 50)), luma: bell(112) });
  },
  folderCover: () => later(images.slice(0, 4)),
  foldersExist: (paths) => later(paths.map(() => true)),
  cacheUsage: () => later({ bytes: 1_250_000_000, count: 31_400, limit: 5 * 1024 ** 3 }),
  trimCache: () => later(0),
  setCacheLimit: () => later(undefined),
  pickFolder: () => later("/Photos/Sample Shoot"),
  reveal: () => later(undefined),
  report: (message) => console.info("[report]", message),
  finishSelfTest: () => {},
  pause: (ms) => later(undefined, ms),
  url: (tier, photo, size) => {
    const i = images.findIndex((im) => im.display === photo.display);
    return picture(Math.max(0, i), tier === "thumb" ? Math.min(size ?? 320, 480) : tier === "screen" ? 1200 : tier === "fit" ? 1600 : 2400);
  },
};
