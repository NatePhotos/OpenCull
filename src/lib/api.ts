// Everything the interface asks of the Rust backend, in one place.
//
// Two implementations sit behind it. Inside the app it is the real backend.
// In a plain browser tab — `npm run dev` with no window around it — there is
// no backend to call, so a stand-in answers with made-up photos. That exists
// only so the layout can be worked on without building the app, and is left
// out of the shipped bundle entirely.

import { Channel, invoke } from "@tauri-apps/api/core";
import type { AiEvent, Compared, FileKinds, Framing, Histogram, Marks, MetaEvent, RunFrame, Scanned } from "./types";

export interface AppInfo {
  platform: string;
  version: string;
  cacheDir: string;
  /** A folder named on the command line, to open straight away. */
  openPath: string | null;
  /** Set when the app was started to check itself rather than to be used. */
  selfTest: boolean;
}

/** A newer version than the one running. */
export interface UpdateFound {
  version: string;
  current: string;
}

export interface FolderReply {
  session: number;
  images: Scanned[];
  subfolderCount: number;
  /** Photos not listed because of which kinds of file were asked for. */
  leftOut: number;
  /** When each photo was taken, where that is known yet. */
  times: (number | null)[];
  /** Every time is known, so the folder can open in the order it was shot. */
  timed: boolean;
  /** Each photo's burst, when `timed`. */
  groups: (number | null)[] | null;
}

export interface CacheUsage {
  bytes: number;
  count: number;
  /** How many bytes the cache is kept under. */
  limit: number;
}

/** What became of photos sent to another folder. */
export interface Shipped {
  /** Where each photo is now, in the order given. */
  placed: string[];
  /** The photos that are no longer where they started. */
  moved: string[];
  skipped: number;
  /** Photos that never arrived, each with why. */
  failed: string[];
  /** Photos that arrived without their ratings, each with why. */
  sidecarFailed: string[];
}

/** A program Ship can open photos in. */
export interface EditingApp {
  id: string;
  name: string;
  installed: boolean;
  takesCatalog: boolean;
}

/** A card or outside drive photos could be ingested from. */
export interface Volume {
  path: string;
  name: string;
  removable: boolean;
  /** Has a camera's folders at its top: DCIM and the like. */
  cameraCard: boolean;
}

/** A folder in the ingest source browser. */
export interface IngestFolder {
  path: string;
  name: string;
  /** Photos directly inside it. */
  imageCount: number;
}

/** What a photo's own file says about it, read after the card is listed. */
export interface IngestDetail {
  index: number;
  captureMs: number | null;
  rating: number;
}

export type IngestPolicy = "uniqueName" | "overwrite" | "skip";

/**
 * How far a copy off a card has got. `bytesCopied` is what was written, for
 * the speed; `bytesSettled` is what is finished either way, skipped files
 * included, for the time left.
 */
export interface IngestProgress {
  done: number;
  total: number;
  bytesCopied: number;
  bytesSettled: number;
  bytesTotal: number;
}

export interface Ingested {
  status: "success" | "cancelled" | "failed";
  message: string | null;
}

export type Tier = "thumb" | "fit" | "screen" | "full";

export interface Backend {
  appInfo(): Promise<AppInfo>;
  openFolder(path: string, includeSubfolders: boolean, kinds: FileKinds, members?: Scanned[], ignoreVideos?: boolean): Promise<FolderReply>;
  closeFolder(session: number): Promise<void>;
  loadMetadata(session: number, onEvent: (e: MetaEvent) => void): Promise<void>;
  prefetch(items: { path: string; version: string }[], size: number): Promise<void>;
  /** Prepares a long run of loupe previews on spare time. Resolves to how many there are to make. */
  warm(items: { path: string; version: string }[], size: number): Promise<number>;
  warmRemaining(): Promise<number>;
  /** Records marks beside each photo, and inside each of the JPEGs named in `into`. */
  setMarks(changes: ({ path: string; into: string[] } & Marks)[]): Promise<void>;
  flushMarks(): Promise<void>;
  /**
   * Records how photos are cropped and turned, beside each photo in the
   * fields editing programs read, and the crop inside each of the JPEGs named
   * in `into`. `upright` is the size of the picture the framing was made on,
   * and `seed` the photo's marks, for a sidecar that has to be made.
   */
  setFraming(changes: { path: string; into: string[]; framing: Framing; upright: [number, number]; seed: Marks }[]): Promise<void>;
  moveToTrash(items: { path: string; display: string }[]): Promise<string[]>;
  /** Copies or moves photos, with their JPEGs and ratings, to another folder. Never replaces anything there. */
  transferPhotos(items: { path: string; display: string }[], destination: string, moving: boolean): Promise<Shipped>;
  /** How many of these photos, or their JPEGs, would land on a file already in `destination`. */
  shipConflicts(items: { path: string; display: string }[], destination: string): Promise<number>;
  /** Writes out pending ratings, then copies or moves the photos, with their JPEGs, if there is a destination. */
  ship(items: { path: string; display: string }[], destination: string | null, moving: boolean, skipDuplicates: boolean, onProgress: (done: number) => void): Promise<Shipped>;
  editingApps(): Promise<EditingApp[]>;
  lightroomCatalogs(): Promise<string[]>;
  openInEditor(editor: string, files: string[], catalog: string | null): Promise<void>;
  pathsExist(paths: string[]): Promise<boolean[]>;
  /**
   * Finds and judges faces, `order` first to last; photos done before come at once. Asking again stops the last asking.
   * `advanced` judges them the newer way; otherwise as the last release did. Each remembers a folder separately.
   */
  aiScan(session: number, order: number[], advanced: boolean, onEvent: (e: AiEvent) => void): Promise<void>;
  /** Tells the scan now running which photos are being looked at, nearest first: it analyses those before the rest. */
  aiLookAt(order: number[]): Promise<void>;
  /**
   * Marks photos against the others of the same pose. Each run is photos
   * taken one soon after another, in the order they were shot; the backend
   * finds the poses in it — or, without `poses`, only the bursts.
   */
  aiCompare(runs: RunFrame[][], poses: boolean): Promise<Compared[][]>;
  aiStop(): Promise<void>;
  /** Whether analysis stands aside while the photographer moves between photos. */
  aiSetYield(on: boolean): Promise<void>;
  /** Bytes remembered of past analysis, and how many folders that is. */
  aiCacheUsage(): Promise<[number, number]>;
  aiClearCache(): Promise<void>;
  /** Asks whether a newer version has been released. The one thing that uses the internet. */
  updateCheck(): Promise<UpdateFound | null>;
  /** Downloads and installs the newest version and reopens the app. Settles only if it could not. */
  updateInstall(onProgress: (done: number, total: number | null) => void): Promise<void>;
  /** Opens a release's page in the browser, or the list of releases. */
  openReleasePage(version: string | null): Promise<void>;
  ingestVolumes(): Promise<Volume[]>;
  ingestBrowse(path: string, ignoreVideos?: boolean): Promise<{ folders: IngestFolder[]; imageCount: number }>;
  ingestScan(path: string, ignoreVideos?: boolean): Promise<Scanned[]>;
  ingestSources(sources: string[], ignoreVideos?: boolean): Promise<Scanned[]>;
  /** Reads capture times and in-camera stars, in batches. Asking again stops the last asking. */
  ingestDetails(paths: string[], onFound: (found: IngestDetail[]) => void): Promise<void>;
  ingestConflicts(items: { path: string; display: string }[], destination: string): Promise<number>;
  ingestCopy(items: { path: string; display: string }[], destination: string, policy: IngestPolicy, onProgress: (p: IngestProgress) => void): Promise<Ingested>;
  ingestCancel(): Promise<void>;
  /** Calls `handler` when quitting was refused because a copy is running. Returns a way to stop listening. */
  onQuitRefused(handler: () => void): () => void;
  /** Calls `handler` when marks could not be written into a JPEG: the file, and why. */
  onJpegNotWritten(handler: (path: string, why: string) => void): () => void;
  /** Calls `handler` when marks could not be saved at all: the photo, and why. */
  onMarksNotSaved(handler: (path: string, why: string) => void): () => void;
  /**
   * Calls `handler` when a photo's framing could not be saved: the file, why,
   * and whether it is saved nowhere (`lost`) or only missing from a JPEG.
   */
  onFramingNotSaved(handler: (path: string, why: string, lost: boolean) => void): () => void;
  histogram(path: string, version: string): Promise<Histogram>;
  folderCover(path: string): Promise<Scanned[]>;
  foldersExist(paths: string[]): Promise<boolean[]>;
  cacheUsage(): Promise<CacheUsage>;
  trimCache(limit: number): Promise<number>;
  setCacheLimit(limit: number): Promise<void>;
  pickFolder(title: string, startAt?: string): Promise<string | null>;
  reveal(path: string): Promise<void>;
  report(message: string): void;
  finishSelfTest(ok: boolean): void;
  pause(ms: number): Promise<void>;
  url(tier: Tier, photo: Pick<Scanned, "display" | "version">, size?: number): string;
}

export const inTauri = "__TAURI_INTERNALS__" in window;

// Windows and Android cannot register a scheme of their own, so there a
// custom protocol is reached as a host under http instead.
const previewBase = /Windows|Android/.test(navigator.userAgent) ? "http://opencull.localhost" : "opencull://localhost";

const tauri: Backend = {
  appInfo: () => invoke("app_info"),
  openFolder: (path, includeSubfolders, kinds, members, ignoreVideos) => invoke("open_folder", { path, includeSubfolders, kinds, members, ignoreVideos }),
  closeFolder: (session) => invoke("close_folder", { session }),
  loadMetadata: (session, onEvent) => {
    const channel = new Channel<MetaEvent>();
    channel.onmessage = onEvent;
    return invoke("load_metadata", { session, onEvent: channel });
  },
  prefetch: (items, size) => invoke("prefetch", { items, size }),
  warm: (items, size) => invoke("warm", { items, size }),
  warmRemaining: () => invoke("warm_remaining"),
  setMarks: (changes) => invoke("set_marks", { changes }),
  flushMarks: () => invoke("flush_marks"),
  setFraming: (changes) => invoke("set_framing", { changes }),
  moveToTrash: (items) => invoke("move_to_trash", { items }),
  transferPhotos: (items, destination, moving) => invoke("transfer_photos", { items, destination, moving }),
  shipConflicts: (items, destination) => invoke("ship_conflicts", { items, destination }),
  ship: (items, destination, moving, skipDuplicates, onProgress) => {
    const progress = new Channel<number>();
    progress.onmessage = onProgress;
    return invoke("ship", { items, destination, moving, skipDuplicates, progress });
  },
  editingApps: () => invoke("editing_apps"),
  lightroomCatalogs: () => invoke("lightroom_catalogs"),
  openInEditor: (editor, files, catalog) => invoke("open_in_editor", { editor, files, catalog }),
  pathsExist: (paths) => invoke("paths_exist", { paths }),
  aiScan: (session, order, advanced, onEvent) => {
    const events = new Channel<AiEvent>();
    events.onmessage = onEvent;
    return invoke("ai_scan", { session, order, advanced, events });
  },
  aiCompare: (runs, poses) => invoke("ai_compare", { runs, poses }),
  aiLookAt: (order) => invoke("ai_look_at", { order }),
  aiStop: () => invoke("ai_stop"),
  aiSetYield: (on) => invoke("ai_set_yield", { on }),
  aiCacheUsage: () => invoke("ai_cache_usage"),
  aiClearCache: () => invoke("ai_clear_cache"),
  updateCheck: () => invoke("update_check"),
  updateInstall: (onProgress) => {
    const progress = new Channel<{ done: number; total: number | null }>();
    progress.onmessage = (p) => onProgress(p.done, p.total);
    return invoke("update_install", { progress });
  },
  openReleasePage: (version) => invoke("open_release_page", { version }),
  ingestVolumes: () => invoke("ingest_volumes"),
  ingestBrowse: (path, ignoreVideos) => invoke("ingest_browse", { path, ignoreVideos }),
  ingestScan: (path, ignoreVideos) => invoke("ingest_scan", { path, ignoreVideos }),
  ingestSources: (sources, ignoreVideos) => invoke("ingest_sources", { sources, ignoreVideos }),
  ingestDetails: (paths, onFound) => {
    const found = new Channel<IngestDetail[]>();
    found.onmessage = onFound;
    return invoke("ingest_details", { paths, found });
  },
  ingestConflicts: (items, destination) => invoke("ingest_conflicts", { items, destination }),
  ingestCopy: (items, destination, policy, onProgress) => {
    const progress = new Channel<IngestProgress>();
    progress.onmessage = onProgress;
    return invoke("ingest_copy", { items, destination, policy, progress });
  },
  ingestCancel: () => invoke("ingest_cancel"),
  onJpegNotWritten: (handler) => {
    let stop: (() => void) | undefined;
    let stopped = false;
    void import("@tauri-apps/api/event").then(({ listen }) =>
      listen<[string, string]>("jpeg-not-written", (event) => handler(event.payload[0], event.payload[1])).then((unlisten) => {
        if (stopped) unlisten();
        else stop = unlisten;
      }),
    );
    return () => {
      stopped = true;
      stop?.();
    };
  },
  onMarksNotSaved: (handler) => {
    let stop: (() => void) | undefined;
    let stopped = false;
    void import("@tauri-apps/api/event").then(({ listen }) =>
      listen<[string, string]>("marks-not-saved", (event) => handler(event.payload[0], event.payload[1])).then((unlisten) => {
        if (stopped) unlisten();
        else stop = unlisten;
      }),
    );
    return () => {
      stopped = true;
      stop?.();
    };
  },
  onFramingNotSaved: (handler) => {
    const stops: (() => void)[] = [];
    let stopped = false;
    void import("@tauri-apps/api/event").then(({ listen }) => {
      for (const [event, lost] of [["framing-not-saved", true], ["crop-not-in-jpeg", false]] as const) {
        void listen<[string, string]>(event, (e) => handler(e.payload[0], e.payload[1], lost)).then((unlisten) => {
          if (stopped) unlisten();
          else stops.push(unlisten);
        });
      }
    });
    return () => {
      stopped = true;
      stops.forEach((stop) => stop());
    };
  },
  onQuitRefused: (handler) => {
    let stop: (() => void) | null = null;
    let stopped = false;
    void import("@tauri-apps/api/event").then(({ listen }) =>
      listen("quit-refused", handler).then((unlisten) => {
        if (stopped) unlisten();
        else stop = unlisten;
      }),
    );
    return () => {
      stopped = true;
      stop?.();
    };
  },
  histogram: (path, version) => invoke("histogram", { path, version }),
  folderCover: (path) => invoke("folder_cover", { path }),
  foldersExist: (paths) => invoke("folders_exist", { paths }),
  cacheUsage: () => invoke("cache_usage"),
  trimCache: (limit) => invoke("trim_cache", { limit }),
  setCacheLimit: (limit) => invoke("set_cache_limit", { limit }),
  pickFolder: async (title, startAt) => {
    const { open } = await import("@tauri-apps/plugin-dialog");
    const picked = await open({ directory: true, multiple: false, title, defaultPath: startAt });
    return typeof picked === "string" ? picked : null;
  },
  reveal: async (path) => {
    const { revealItemInDir } = await import("@tauri-apps/plugin-opener");
    await revealItemInDir(path);
  },
  report: (message) => void invoke("report", { message }).catch(() => {}),
  finishSelfTest: (ok) => void invoke("finish_self_test", { ok }).catch(() => {}),
  pause: (ms) => invoke("pause", { ms }),
  url: (tier, photo, size) => {
    const s = size ? `s=${size}&` : "";
    return `${previewBase}/${tier}?${s}v=${photo.version}&p=${encodeURIComponent(photo.display)}`;
  },
};

let backend: Backend = tauri;
if (!inTauri) {
  if (!import.meta.env.DEV) throw new Error("OpenCull has to be run as an app, not opened in a browser.");
  backend = (await import("./mock")).mock;
}

export const api = backend;

/** The sizes thumbnails are kept at. Matches the backend's list. */
export const THUMB_SIZES = [320, 640, 1280] as const;
/** The sizes loupe previews are kept at. Matches the backend's list. */
export const FIT_SIZES = [1600, 2400, 3400] as const;

/** The smallest kept size that fills `pixels`, or the largest there is. */
export function bucket(sizes: readonly number[], pixels: number): number {
  return sizes.find((s) => s >= pixels) ?? sizes[sizes.length - 1];
}
