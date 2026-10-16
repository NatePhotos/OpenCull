import { validMobileClipGrouping, type MobileClipGrouping } from "./mobileclip-grouping";
import { validRankingPalette, type RankingPalette } from "./ranking-palette";
import { isVideo } from "./media";
import { hasMotionBlur, motionAdjustedScore } from "./motion-blur";
// The state of the app: which folder is open, what has been decided about
// each photo in it, and what is on screen.

import { create } from "zustand";
import { api, type UpdateFound } from "./api";
import { useCrop } from "./crop";
import { cropFromShare, isPlain, MIRROR_ACROSS, MIRROR_DOWN, PLAIN, QUARTER_LEFT, QUARTER_RIGHT, sameFraming, shareOf, turned, viewSize, type Share } from "./framing";
import { sourceWritable, type ReviewSession } from "./review";
import { saveReview } from "./review-api";
import { load, save, snapshotPreferences } from "./prefs";
import { shouldShowOverview } from "./onboarding";
import { openingView, validViewMode } from "./view-preferences";
import type { AiFace, AiResult, Color, Compared, FileKinds, Flag, FlagFilter, Framing, Marks, MetaEvent, Photo, Rank, SortKey, ViewMode } from "./types";

interface Session {
  id: number;
  folder: string;
  name: string;
  includeSubfolders: boolean;
  subfolderCount: number;
  /** Which kinds of file it was opened for, and how many photos that left out. */
  fileKinds: FileKinds;
  leftOut: number;
  /** The folder opened already in the order it was shot. */
  timed: boolean;
}

/** One undoable step: every photo it touched, before and after. */
/**
 * One thing done, to be undone as one: each photo it touched, with its marks
 * before and after — or, where it was the framing that changed, that.
 */
type Edit = { index: number; before: Marks; after: Marks; framing?: { before: Framing; after: Framing } }[];

export type ZoomCommand = { kind: "toggle" | "centre" | "face" | "in" | "out"; nonce: number };

interface State {
  platform: string;
  selfTest: boolean;
  /** This app's version, as its release is numbered. */
  version: string;

  session: Session | null;
  review: ReviewSession | null;
  /** The folder is still being listed. */
  loading: boolean;
  /** How much of the folder's ratings and EXIF has been read, 0–1. */
  metaProgress: number;
  openError: string | null;

  photos: Photo[];
  /** Every photo, in the order shown. */
  order: number[];
  /** `order`, less whatever the filters hide. */
  view: number[];
  /** The photo in the loupe, as an index into `photos`; -1 for none. */
  current: number;
  /** Where a shift-click range starts from. */
  anchor: number;
  selected: Set<number>;

  flagFilter: FlagFilter;
  ratingFilter: number | null;
  motionBlurFilter: boolean;
  colorFilters: Set<Color>;
  sort: SortKey;
  sortAscending: boolean;

  viewMode: ViewMode;
  defaultViewMode: ViewMode;
  rememberViewMode: boolean;
  gridColumns: number;
  showExif: boolean;
  showHistogram: boolean;
  /** How wide the EXIF panel is, in pixels. */
  exifPanelWidth: number;
  showFilmstrip: boolean;
  showInfo: boolean;
  showRatingBar: boolean;
  showFilterBar: boolean;
  rankingPalette: RankingPalette;
  experimentalNativeFilmstrip: boolean;
  experimentalSmoothZoom: boolean;
  experimentalTransitions: boolean;
  experimentalHideFilmstripOnCrop: boolean;
  filmstripHeight: number;
  sceneStripHeight: number;
  /** The strip's edge is being dragged, and its height is changing every frame. */
  stripResizing: boolean;
  /** A side panel's edge is being dragged, and the photo beside it changes size every frame. */
  panelResizing: boolean;
  includeSubfolders: boolean;
  /** Which of a folder's files are shown as its photos. */
  fileKinds: FileKinds;
  /** Write a JPEG's marks into the file itself, as well as beside it. */
  embedInJpeg: boolean;
  /** Ask before photos go to the trash. */
  askBeforeTrash: boolean;
  /** Look for a newer version each time the app opens. */
  checkForUpdates: boolean;
  /** A version the photographer chose to pass over. It is not offered again
   *  unasked; the one after it is. */
  skippedUpdate: string;
  /** A newer version that has been found, if one has. */
  updateFound: UpdateFound | null;
  /** The offer to install it is on screen. */
  updateOpen: boolean;
  ignoreVideos: boolean;
  /** Move on to the next photo after marking one. Follows Caps Lock. */
  speedCull: boolean;
  /** The same, left on whatever Caps Lock is doing. */
  speedCullAlways: boolean;
  /** Carry the zoom, and the spot looked at, from one photo to the next. */
  keepZoom: boolean;
  /** Fetch the full-size picture of a photo rested on, so zooming is immediate. */
  instantZoom: boolean;
  /** Space's zoom target uses the highest scored face rather than the centre. */
  spaceZoomToFace: boolean;
  experimentalFaceOutlines: boolean;
  experimentalFaceSpotlight: boolean;
  /** Draw the histogram as red, green and blue, not as brightness alone. */
  rgbHistogram: boolean;
  /** How large the preview cache may grow, in gigabytes. */
  cacheLimitGb: number;
  /** Prepare the folder's loupe previews in the background while idle. */
  warmAhead: boolean;
  /** How many of those are still to make. */
  warmRemaining: number;
  /** How many there were to make when this round of preparing began. */
  warmTotal: number;
  /** The loupe preview size the window currently calls for. */
  fitSize: number;

  /** What face analysis found, by photo index; undefined until a photo is analysed. */
  ai: (AiResult | undefined)[];
  /**
   * Each photo marked again as a frame of its burst, or of a pose shot
   * again — the same people set beside themselves from frame to frame —
   * once every photo taken around it is analysed.
   */
  compared: (Compared | undefined)[];
  /** Each photo's place among the frames it was compared with, by those marks. */
  ranks: (Rank | undefined)[];
  /**
   * The pose each compared photo belongs to: a number shared by the frames
   * it was compared with, null for one that stands alone, undefined until
   * the photos around it are analysed. See `groupOf`.
   */
  poses: (number | null | undefined)[];
  /**
   * Group the same pose shot again — the same people in the same places a
   * few seconds on — as well as the frames of one press of the shutter.
   */
  smartGroups: boolean;
  aiMobileClipGrouping: boolean;
  mobileClipGrouping: MobileClipGrouping;
  groupingRevision: number;
  aiGroupingBusy: number;
  aiGroupingError: string | null;
  /** How many photos have been analysed, and whether analysis is still going. */
  aiDone: number;
  aiRunning: boolean;
  aiCached: number;
  aiPending: number | null;
  aiMotionPending: number;
  /** Find and judge faces at all. */
  aiEnabled: boolean;
  /** Judge them the newer way, still being worked on, rather than as the last release did. */
  aiAdvanced: boolean;
  aiMotionBlur: boolean;
  autoRateMotionBlur: boolean;
  motionRatingSkipped: Set<number>;
  /** Show faces judged to be background in the face panel, dimmed. */
  showBackgroundFaces: boolean;
  /** Analysis stands aside whenever a photo is being moved to. */
  aiPause: boolean;
  /** The panel of faces beside the photo. */
  showFaces: boolean;
  /** How wide the face panel is, in pixels. */
  facePanelWidth: number;
  /** A face to zoom the loupe to; numbered so the same one can be asked for twice. */
  faceZoom: { x: number; y: number; nonce: number } | null;
  /** Transient preview only; never saved or applied to another photo. */
  hoveredFace: { photoIndex: number; display: string; version: string; face: AiFace } | null;

  shortcutsOpen: boolean;
  overviewOpen: boolean;
  settingsOpen: boolean;
  shipOpen: boolean;
  /** A photo is being cropped in the big view. */
  cropOpen: boolean;
  /** A crop copied from one photo, to be put on others. */
  copiedCrop: Share | null;
  ingestOpen: boolean;
  /** A card to go straight to when ingest opens: the one just plugged in. */
  ingestFrom: string | null;
  /** Something the photographer has to be told, shown until acknowledged. */
  notice: { title: string; body: string } | null;
  /** Photos waiting on a yes or no before they go to the trash. */
  trashPending: number[] | null;
  zoom: ZoomCommand;

  recents: string[];
  undoStack: Edit[];
  redoStack: Edit[];
}

interface Actions {
  init(): Promise<void>;
  openFolder(path: string, review?: ReviewSession): Promise<void>;
  closeFolder(): void;
  reloadForSubfolders(): void;

  select(index: number, mods: { meta: boolean; shift: boolean }): void;
  selectAll(): void;
  go(index: number): void;
  next(): void;
  previous(): void;
  nextGroup(): void;
  previousGroup(): void;

  setRating(rating: number, targets?: number[]): void;
  setFlag(flag: Flag, targets?: number[]): void;
  setColor(color: Color, targets?: number[]): void;
  clearRatingAndColor(targets?: number[]): void;
  /** Crops or turns one photo. */
  setFraming(index: number, framing: Framing): void;
  /** Turns photos a quarter left or right, or mirrors them across or down. */
  turnPhotos(how: "left" | "right" | "across" | "down", targets?: number[]): void;
  /** Takes the crop and any turn off photos. */
  resetFraming(targets?: number[]): void;
  /** Copies a photo's crop: the one given, or the one being looked at. */
  copyCrop(index?: number): void;
  /** Puts the copied crop on photos: those given, the selection, or the one being looked at. */
  pasteCrop(targets?: number[]): void;
  undo(): void;
  redo(): void;

  setFlagFilter(f: FlagFilter): void;
  setRatingFilter(r: number | null): void;
  setMotionBlurFilter(on: boolean): void;
  rateMotionBlur(): void;
  toggleColorFilter(c: Color): void;
  clearFilters(): void;
  setSort(key: SortKey, ascending: boolean): void;

  set<K extends keyof State>(key: K, value: State[K]): void;
  toggle(key: "showExif" | "showFilmstrip" | "showInfo" | "showRatingBar" | "showFilterBar" | "showFaces"): void;
  /** Compares every analysed photo afresh, after a change to what counts as a group. */
  regroup(): void;
  /** Takes in a batch of analysed photos, and compares the frames of any pose they complete. */
  addAnalysis(items: ({ index: number } & AiResult)[]): void;
  /** Zooms the loupe to a face. */
  zoomToFace(face: Pick<AiFace, "x" | "y" | "w" | "h">): void;
  zoomCommand(kind: ZoomCommand["kind"]): void;

  requestTrash(targets?: number[]): void;
  confirmTrash(): Promise<void>;
  /** Asks for a folder, then copies or moves these photos into it. */
  transfer(targets: number[], moving: boolean): Promise<void>;
  /** Opens Ship — on these photos, if any are given. */
  openShip(targets?: number[]): void;
  /** Begins cropping a photo: the one given, or the one being looked at. */
  openCrop(index?: number): void;
  /** Keeps the crop as it stands, and carries on cropping. */
  acceptCrop(): void;
  /** Ends cropping, keeping what was made or leaving the photo as it was. */
  closeCrop(accept: boolean): void;
  /** Takes photos that have left the folder out of what is shown. */
  forget(paths: string[]): void;

  removeRecents(paths: string[]): void;
  relocateRecent(from: string, to: string): void;
}

export type Store = State & Actions;

export const GIGABYTE = 1024 ** 3;

/**
 * Which group a photo belongs to, or null if it stands alone: what the scene
 * view, the filmstrip and moving by group all go by.
 *
 * Its pose, once the photos around it have been analysed and compared; the
 * press of the shutter it came from until then. So a folder being scanned
 * starts out grouped by the clock and settles into poses as the scan goes.
 * Poses are numbered below nought so the two can never be mistaken.
 */
export function groupOf(s: Pick<State, "photos" | "poses">, index: number): number | null {
  const pose = s.poses[index];
  return pose !== undefined ? pose : (s.photos[index]?.group ?? null);
}

/** Whether a photo is taller than it is wide, the way up it is shown; null until its shape has been read. */
export function uprightOf(photo: Photo): boolean | null {
  return photo.fullWidth > 0 && photo.fullHeight > 0 ? photo.fullHeight > photo.fullWidth : null;
}

/**
 * Photos further apart than this are never compared: they cannot be the same
 * pose. The backend holds the same figure and does the deciding; this only
 * settles how much it is sent at once.
 */
const RUN_GAP_MS = 3_000;

/**
 * The folder cut into runs of photos taken one soon after another, each in
 * the order it was shot. A photo with no capture time is in none.
 */
export function runsOf(photos: Photo[]): number[][] {
  const timed = photos.filter((p) => p.captureMs !== null).sort((a, b) => a.captureMs! - b.captureMs! || a.index - b.index);
  const runs: number[][] = [];
  timed.forEach((p, at) => {
    if (at > 0 && p.captureMs! - timed[at - 1].captureMs! <= RUN_GAP_MS) runs[runs.length - 1].push(p.index);
    else runs.push([p.index]);
  });
  return runs;
}

/**
 * Ranks the frames of each pose in a run against one another, best first,
 * writing into `ranks`. A frame compared with nothing has no rank.
 */
export function rankRun(ranks: (Rank | undefined)[], members: number[], compared: Compared[]): void {
  const poses = new Map<number, number[]>();
  members.forEach((_, at) => {
    const pose = poses.get(compared[at].pose);
    if (pose) pose.push(at);
    else poses.set(compared[at].pose, [at]);
  });
  for (const frames of poses.values()) {
    if (frames.length < 2) {
      delete ranks[members[frames[0]]];
      continue;
    }
    // Best mark first; a frame with no mark last; ties in shooting order.
    const ordered = frames.slice().sort((a, b) => (compared[b].score ?? -1) - (compared[a].score ?? -1));
    ordered.forEach((at, place) => (ranks[members[at]] = { rank: place + 1, count: frames.length }));
  }
}

/**
 * Eyes this much less open than the same person's best in the burst are a
 * blink, or half of one. MEASURED by eye on 200 such comparisons: under
 * this nearly every one is; above it, laughing and looking down creep in.
 */
export const BLINK = 0.6;

/**
 * Whether eyes this far short of the same person's best are a blink: not
 * where the face's state gives a reason for them — looking down, a laugh, a
 * head turned away.
 */
export const blinked = (face: AiFace, share: number | null | undefined) => share != null && share < BLINK && !face.excused;

export type Problem = "motionBlur" | "eyesClosed" | "blinking" | "lookingAway" | "softFocus";
export const PROBLEM_LABEL: Record<Problem, string> = { motionBlur: "Motion blur", eyesClosed: "Eyes closed", blinking: "Blinking", lookingAway: "Looking away", softFocus: "Soft focus" };

/** The worst thing wrong with a photo's subjects, if anything is. */
export function problemOf(result: AiResult | undefined, compared?: Compared, motionBlur = false): Problem | null {
  if (motionBlur && hasMotionBlur(result)) return "motionBlur";
  const subjects = result?.faces.filter((f) => f.subject) ?? [];
  if (subjects.some((f) => f.eyesShut)) return "eyesClosed";
  if (result?.faces.some((f, i) => f.subject && blinked(f, compared?.eyes[i]))) return "blinking";
  if (subjects.some((f) => f.lookingAway)) return "lookingAway";
  if (subjects.some((f) => f.sharpness !== null && f.sharpness < 50)) return "softFocus";
  return null;
}

/** Whether marking a photo should move on to the next one. */
/** Presentation only: leave the saved filmstrip preference untouched while cropping. */
export const filmstripVisible = (s: Pick<State, "showFilmstrip" | "cropOpen" | "experimentalHideFilmstripOnCrop">) => s.showFilmstrip && !(s.cropOpen && s.experimentalHideFilmstripOnCrop);

export const speedCulling = (s: Pick<State, "speedCull" | "speedCullAlways">) => s.speedCull || s.speedCullAlways;

/** Settings written back to storage whenever they change. */
const PERSISTED = [
  "viewMode",
  "defaultViewMode",
  "rememberViewMode",
  "sort",
  "sortAscending",
  "gridColumns",
  "showExif",
  "showHistogram",
  "exifPanelWidth",
  "showFilmstrip",
  "showInfo",
  "showRatingBar",
  "showFilterBar",
  "rankingPalette",
  "experimentalNativeFilmstrip",
  "experimentalSmoothZoom",
  "experimentalTransitions",
  "experimentalHideFilmstripOnCrop",
  "filmstripHeight",
  "sceneStripHeight",
  "includeSubfolders",
  "ignoreVideos",
  "fileKinds",
  "embedInJpeg",
  "askBeforeTrash",
  "checkForUpdates",
  "skippedUpdate",
  "warmAhead",
  "speedCullAlways",
  "keepZoom",
  "instantZoom",
  "spaceZoomToFace",
  "experimentalFaceOutlines",
  "experimentalFaceSpotlight",
  "rgbHistogram",
  "cacheLimitGb",
  "aiEnabled",
  "aiAdvanced",
  "aiMotionBlur",
  "autoRateMotionBlur",
  "showBackgroundFaces",
  "aiPause",
  "smartGroups",
  "aiMobileClipGrouping",
  "mobileClipGrouping",
  "showFaces",
  "facePanelWidth",
  "recents",
] as const satisfies readonly (keyof State)[];

const MAX_RECENTS = 24;
const MAX_UNDO = 200;

function baseName(path: string): string {
  const parts = path.split(/[\\/]/).filter(Boolean);
  return parts[parts.length - 1] ?? path;
}

function marksOf(p: Photo): Marks {
  return { rating: p.rating, flag: p.flag, color: p.color };
}

type FilterState = Pick<State, "flagFilter" | "ratingFilter" | "colorFilters" | "motionBlurFilter" | "aiMotionBlur" | "aiEnabled" | "ai">;

function hasFilter(s: FilterState): boolean {
  return s.flagFilter !== "all" || s.ratingFilter !== null || s.colorFilters.size > 0 || s.motionBlurFilter;
}

export function hasActiveFilter(s: State): boolean {
  return hasFilter(s);
}

/** Whether the filters let this photo through. Every active one must agree. */
function passes(p: Photo, s: FilterState): boolean {
  if (s.flagFilter === "picks" && p.flag !== "pick") return false;
  if (s.flagFilter === "rejects" && p.flag !== "reject") return false;
  if (s.flagFilter === "unflagged" && p.flag !== "none") return false;
  if (s.ratingFilter !== null && p.rating !== s.ratingFilter) return false;
  if (s.colorFilters.size > 0 && !s.colorFilters.has(p.color)) return false;
  if (s.motionBlurFilter && (!s.aiEnabled || !s.aiMotionBlur || !hasMotionBlur(s.ai[p.index]))) return false;
  return true;
}

function filtered(order: number[], photos: Photo[], s: FilterState): number[] {
  return hasFilter(s) ? order.filter((i) => passes(photos[i], s)) : order;
}

/**
 * The order photos are shown in.
 *
 * A photo's index is its place in the folder scan, which is folder then
 * filename — so sorting by filename is the indices in order, and every other
 * sort breaks its ties on the index to keep same-second frames in shot order.
 */
function sorted(photos: Photo[], key: SortKey, ascending: boolean): number[] {
  const order = photos.map((p) => p.index);
  const dir = ascending ? 1 : -1;
  if (key === "filename") return ascending ? order : order.reverse();
  if (key === "rating") {
    return order.sort((a, b) => (photos[a].rating - photos[b].rating) * dir || a - b);
  }
  // A frame with no readable time sorts as though taken before everything.
  const time = (i: number) => photos[i].captureMs ?? -Infinity;
  return order.sort((a, b) => {
    const ta = time(a);
    const tb = time(b);
    return ta === tb ? a - b : (ta < tb ? -1 : 1) * dir;
  });
}

export const useStore = create<Store>((set, get) => {
  /** Applies new marks to photos, records the step, and writes the sidecars. */
  function applyMarks(changes: { index: number; after: Marks }[], record = true) {
    const s = get();
    const edit: Edit = [];
    const photos = s.photos.slice();
    for (const { index, after } of changes) {
      const before = marksOf(photos[index]);
      if (before.rating === after.rating && before.flag === after.flag && before.color === after.color) continue;
      edit.push({ index, before, after });
      photos[index] = { ...photos[index], ...after };
    }
    if (edit.length === 0) return;

    // A JPEG keeps its marks inside itself, where other programs look for
    // them: the photo's own file if it is one, and the JPEG it is drawn from
    // if it is a RAW with one beside it.
    const jpegs = (p: Photo) => (s.embedInJpeg ? [...new Set([p.path, p.display])].filter((file) => /\.jpe?g$/i.test(file)) : []);
    if (sourceWritable(s.review)) void api.setMarks(edit.map((e) => ({ path: photos[e.index].path, into: jpegs(photos[e.index]), ...e.after })));
    set({
      photos,
      ...(s.review ? { review: { ...s.review, decisions: { ...s.review.decisions, ...Object.fromEntries(edit.map((e) => { const p = photos[e.index]; return [p.path, { rating: p.rating, flag: p.flag, color: p.color, framing: p.framing }]; })) } } } : {}),
      // A photo that no longer passes the filter leaves the view at once,
      // which is what filtering to "unrated" and rating your way through
      // the folder relies on.
      view: filtered(s.order, photos, s),
      ...(record ? { undoStack: [...s.undoStack.slice(-MAX_UNDO + 1), edit], redoStack: [] } : {}),
    });
  }

  /**
   * Crops and turns photos, and has it written beside them. One step to
   * undo, however many photos. A photo that cannot be framed here — its
   * sidecar cannot be followed, or its picture is not the whole of it — is
   * passed over.
   */
  function applyFraming(changes: { index: number; framing: Framing }[], record = true) {
    const s = get();
    const edit: Edit = [];
    const photos = s.photos.slice();
    for (const { index, framing } of changes) {
      const photo = photos[index];
      if (!photo || !photo.framable || photo.fullWidth <= 0 || sameFraming(photo.framing, framing)) continue;
      const marks = marksOf(photo);
      edit.push({ index, before: marks, after: marks, framing: { before: photo.framing, after: framing } });
      photos[index] = { ...photo, framing: isPlain(framing) ? PLAIN : framing };
    }
    if (edit.length === 0) return;

    // A self-test frames nothing in anyone's folder.
    if (!s.selfTest && sourceWritable(s.review)) {
      const jpegs = (p: Photo) => (s.embedInJpeg ? [...new Set([p.path, p.display])].filter((file) => /\.jpe?g$/i.test(file)) : []);
      void api.setFraming(
        edit.map((e) => {
          const p = photos[e.index];
          return { path: p.path, into: jpegs(p), framing: p.framing, upright: [p.fullWidth, p.fullHeight] as [number, number], seed: e.after };
        }),
      );
    }
    set({ photos, ...(s.review ? { review: { ...s.review, decisions: { ...s.review.decisions, ...Object.fromEntries(edit.map((e) => { const p = photos[e.index]; return [p.path, { rating: p.rating, flag: p.flag, color: p.color, framing: p.framing }]; })) } } } : {}), ...(record ? { undoStack: [...s.undoStack.slice(-MAX_UNDO + 1), edit], redoStack: [] } : {}) });
  }

  /**
   * Where undoing or redoing a step leaves the loupe: on the photo the step
   * changed, so that what was just put back is what is being looked at. A
   * step that changed many lands on the first of them as they are shown. It
   * is gone to even if the filter hides it — it is what was asked about.
   */
  function backTo(edit: Edit): Partial<State> {
    const { order, current } = get();
    const touched = new Set(edit.map((e) => e.index));
    const first = order.find((i) => touched.has(i));
    return first === undefined || first === current ? {} : { current: first, anchor: first };
  }

  /** The photos a keyboard mark lands on: just the one in the loupe. */
  function currentOnly(): number[] {
    const { current } = get();
    return current >= 0 ? [current] : [];
  }

  /**
   * The nearest photo in the filtered view, `step` places from the current
   * one. Works even when the current photo has just been filtered out from
   * under the cursor, by finding where it would have been.
   */
  function neighbour(step: 1 | -1): number {
    const { view, order, current } = get();
    if (view.length === 0) return -1;
    const at = view.indexOf(current);
    if (at >= 0) return view[Math.min(view.length - 1, Math.max(0, at + step))];

    const visible = new Set(view);
    let pos = order.indexOf(current);
    for (pos += step; pos >= 0 && pos < order.length; pos += step) {
      if (visible.has(order[pos])) return order[pos];
    }
    return step > 0 ? view[view.length - 1] : view[0];
  }

  /**
   * Applies a change of filter, and keeps the loupe on something it shows.
   *
   * Choosing "rejects" while looking at a pick would otherwise leave the pick
   * in the loupe above a filmstrip that no longer contains it. The loupe moves
   * to the nearest photo after it that passes, or failing that the nearest
   * before — the same place as if the hidden photo had been stepped off.
   */
  function refilter(change: Partial<State>, view: number[]) {
    const { order, current } = get();
    let next = current;
    if (view.length > 0 && !view.includes(current)) {
      const visible = new Set(view);
      const at = order.indexOf(current);
      next = order.slice(at + 1).find((i) => visible.has(i)) ?? [...order.slice(0, Math.max(0, at))].reverse().find((i) => visible.has(i)) ?? view[0];
    }
    set({ ...change, view, current: next, ...(next !== current ? { anchor: next, selected: new Set<number>() } : {}) });
  }

  /** Photos on their way to the trash. Without the question to answer, the
   *  key can be pressed again before the first press has landed; this keeps
   *  the same photo from being sent twice. */
  const trashing = new Set<number>();

  /** Sends photos to the trash, and takes those that went out of what is shown. */
  async function trash(indices: number[]) {
    const s = get();
    if (!s.session) return;
    const session = s.session.id;
    const going = indices.filter((i) => !trashing.has(i));
    if (going.length === 0) return;
    going.forEach((i) => trashing.add(i));
    try {
      const failed = new Set(await api.moveToTrash(going.map((i) => ({ path: s.photos[i].path, display: s.photos[i].display }))));
      // The folder may have been closed, or another opened, while this ran.
      if (get().session?.id === session) get().forget(going.filter((i) => !failed.has(s.photos[i].path)).map((i) => s.photos[i].path));
    } finally {
      going.forEach((i) => trashing.delete(i));
    }
  }

  /** Takes photos that are no longer in the folder out of what is shown. */
  function drop(gone: Set<number>) {
    if (gone.size === 0) return;
    // Land on the photo after the last one removed, or the one before if
    // that was the end of the folder — never back at the start.
    const now = get();
    const survivors = now.view.filter((i) => !gone.has(i));
    let current = now.current;
    if (gone.has(current)) {
      const at = now.view.indexOf(current);
      current = now.view.slice(at + 1).find((i) => !gone.has(i)) ?? [...now.view.slice(0, at)].reverse().find((i) => !gone.has(i)) ?? -1;
    }

    // Photos keep their index for the life of the folder, so the removed
    // ones are dropped from what is shown rather than from the list itself.
    set({
      order: now.order.filter((i) => !gone.has(i)),
      view: survivors,
      current,
      anchor: Math.max(0, current),
      selected: new Set([...now.selected].filter((i) => !gone.has(i))),
      // Undoing a rating on a photo that has gone would recreate its sidecar
      // beside nothing.
      undoStack: [],
      redoStack: [],
    });
  }

  // The open folder's runs, worked out once: capture times are all in
  // before any photo is analysed.
  let runs: { session: number; list: number[][]; of: Map<number, number> } | null = null;

  /**
   * Compares the frames of every run one of these photos belongs to, if all
   * of the run is now analysed: the same people set beside themselves, in a
   * burst or in a pose shot again. Not before then — a rank that changes as
   * the scan reaches the rest would have the photographer picking "the
   * best" of three frames out of seven.
   */
  function compareRuns(touched: number[]) {
    const { photos, ai, session, smartGroups, groupingRevision, aiMobileClipGrouping, aiEnabled, mobileClipGrouping } = get();
    const mobileClip = aiMobileClipGrouping && aiEnabled;
    if (!session || get().metaProgress < 1) return;
    if (runs?.session !== session.id) {
      const list = runsOf(photos).filter((run) => run.length >= 2);
      const of = new Map<number, number>();
      list.forEach((run, at) => run.forEach((index) => of.set(index, at)));
      runs = { session: session.id, list, of };
    }
    const { list, of } = runs;
    const ready = [...new Set(touched.map((i) => of.get(i)).filter((at) => at !== undefined))].map((at) => list[at]).filter((run) => run.every((i) => ai[i] !== undefined));
    if (ready.length === 0) return;
    if (mobileClip) set((s) => ({ aiGroupingBusy: s.aiGroupingBusy + 1 }));
    const live = () => { const s = get(); return s.session?.id === session!.id && s.groupingRevision === groupingRevision && s.aiMobileClipGrouping === aiMobileClipGrouping && s.mobileClipGrouping === mobileClipGrouping && s.aiEnabled === aiEnabled && s.smartGroups === smartGroups; };
    void api
      .aiCompare(ready.map((run) => run.map((i) => ({ ...ai[i]!, index: i, time: photos[i].captureMs, burst: photos[i].group, upright: uprightOf(photos[i]) }))), smartGroups, { session: session.id, revision: groupingRevision, mobileClip, mobileClipGrouping })
      .then((marked) => {
        const now = get();
        // A folder since closed, or the setting changed while this was asked.
        if (!live()) return;
        const compared = now.compared.slice();
        const ranks = now.ranks.slice();
        const poses = now.poses.slice();
        ready.forEach((run, r) => {
          const adjusted = marked[r].map((frame, at) => ({ ...frame, score: motionAdjustedScore(frame.score, now.ai[run[at]], now.aiEnabled && now.aiMotionBlur) }));
          run.forEach((index, at) => (compared[index] = adjusted[at]));
          rankRun(ranks, run, adjusted);
          // A pose is known through the folder by its first frame.
          const first = new Map<number, number>();
          const size = new Map<number, number>();
          run.forEach((index, at) => {
            const pose = marked[r][at].pose;
            if (!first.has(pose)) first.set(pose, index);
            size.set(pose, (size.get(pose) ?? 0) + 1);
          });
          run.forEach((index, at) => {
            const pose = marked[r][at].pose;
            poses[index] = size.get(pose)! > 1 ? -1 - first.get(pose)! : null;
          });
        });
        set({ compared, ranks, poses, aiGroupingError: null });
      })
      .catch((error: unknown) => { if (mobileClip && live()) set({ aiGroupingError: String(error) }); })
      .finally(() => { if (mobileClip && live()) set((s) => ({ aiGroupingBusy: Math.max(0,s.aiGroupingBusy-1) })); });
  }

  function rememberPosition() {
    const { session, photos, current } = get();
    if (session && current >= 0) save(`position.${session.folder}`, photos[current].path);
  }

  return {
    platform: "",
    selfTest: false,
    version: "",
    session: null,
    review: null,
    loading: false,
    metaProgress: 0,
    openError: null,
    photos: [],
    order: [],
    view: [],
    current: -1,
    anchor: 0,
    selected: new Set(),

    flagFilter: "all",
    ratingFilter: null,
    motionBlurFilter: false,
    colorFilters: new Set(),
    sort: load<SortKey>("sort", "captureTime"),
    sortAscending: load("sortAscending", true),

    defaultViewMode: validViewMode(load("defaultViewMode", "loupe")),
    rememberViewMode: load("rememberViewMode", true) === true,
    viewMode: openingView(load("defaultViewMode", "loupe"), load("rememberViewMode", true), load("viewMode", null)),
    gridColumns: load("gridColumns", 4),
    showExif: load("showExif", false),
    showHistogram: load("showHistogram", true),
    exifPanelWidth: load("exifPanelWidth", 250),
    showFilmstrip: load("showFilmstrip", true),
    showInfo: load("showInfo", false),
    showRatingBar: load("showRatingBar", true),
    showFilterBar: load("showFilterBar", false),
    rankingPalette: validRankingPalette(load("rankingPalette", "classic")),
    experimentalNativeFilmstrip: load<boolean>("experimentalNativeFilmstrip", false) === true,
    experimentalHideFilmstripOnCrop: load("experimentalHideFilmstripOnCrop", false),
    experimentalTransitions: load("experimentalTransitions", false),
    experimentalSmoothZoom: load<boolean>("experimentalSmoothZoom", false) === true,
    sceneStripHeight: load("sceneStripHeight", 116),
    filmstripHeight: load("filmstripHeight", 120),
    stripResizing: false,
    panelResizing: false,
    includeSubfolders: load("includeSubfolders", false),
    ignoreVideos: load("ignoreVideos", true),
    fileKinds: load<FileKinds>("fileKinds", "both"),
    // Off by default: rating should not rewrite original files unless asked.
    embedInJpeg: load("embedInJpeg", false),
    askBeforeTrash: load("askBeforeTrash", true),
    checkForUpdates: load("checkForUpdates", true),
    skippedUpdate: load("skippedUpdate", ""),
    updateFound: null,
    updateOpen: false,
    speedCull: false,
    speedCullAlways: load("speedCullAlways", false),
    keepZoom: load("keepZoom", true),
    instantZoom: load("instantZoom", true),
    spaceZoomToFace: load("spaceZoomToFace", false),
    experimentalFaceOutlines: load("experimentalFaceOutlines", false),
    experimentalFaceSpotlight: load("experimentalFaceSpotlight", false),
    rgbHistogram: load("rgbHistogram", true),
    cacheLimitGb: load("cacheLimitGb", 5),
    warmAhead: load("warmAhead", true),
    warmRemaining: 0,
    warmTotal: 0,
    fitSize: 0,

    ai: [],
    compared: [],
    ranks: [],
    poses: [],
    smartGroups: load("smartGroups", true),
    aiMobileClipGrouping: load("aiMobileClipGrouping", false),
    mobileClipGrouping: validMobileClipGrouping(load("mobileClipGrouping", "normal")),
    groupingRevision: 0,
    aiGroupingBusy: 0,
    aiGroupingError: null,
    aiDone: 0,
    aiCached: 0,
    aiPending: null,
    aiMotionPending: 0,
    aiRunning: false,
    aiEnabled: load("aiEnabled", true),
    aiAdvanced: load("aiAdvanced", false),
    aiMotionBlur: load("aiMotionBlur", false),
    autoRateMotionBlur: load("autoRateMotionBlur", true),
    motionRatingSkipped: new Set(),
    showBackgroundFaces: load("showBackgroundFaces", false),
    aiPause: load("aiPause", true),
    showFaces: load("showFaces", true),
    facePanelWidth: load("facePanelWidth", 340),
    faceZoom: null,
    hoveredFace: null,

    shortcutsOpen: false,
    overviewOpen: shouldShowOverview(snapshotPreferences()),
    settingsOpen: false,
    shipOpen: false,
    cropOpen: false,
    copiedCrop: null,
    ingestOpen: false,
    ingestFrom: null,
    notice: null,
    trashPending: null,
    zoom: { kind: "toggle", nonce: 0 },

    recents: load<string[]>("recents", []),
    undoStack: [],
    redoStack: [],

    async init() {
      const info = await api.appInfo();
      set({ platform: info.platform, selfTest: info.selfTest, version: info.version });
      // Asked in the background, and no matter if it cannot be: there may be
      // no connection, and the app is none the worse for not knowing.
      if (!info.selfTest && get().checkForUpdates) {
        void api.updateCheck().then(
          (found) => found && found.version !== get().skippedUpdate && set({ updateFound: found }),
          () => {},
        );
      }
      // A self-test's cache is its own and thrown away; it has no limit to keep.
      if (!info.selfTest) void api.setCacheLimit(get().cacheLimitGb * GIGABYTE).catch(() => {});
      if (info.selfTest && info.openPath) {
        const { runSelfTest } = await import("./selftest");
        void runSelfTest(info.openPath);
      }
      if (info.openPath) await get().openFolder(info.openPath);
    },

    async openFolder(path, review) {
      const before = get();
      if (before.session) get().closeFolder();

      const { includeSubfolders, fileKinds } = get();
      set({
        review: review ?? null,
        session: { id: -1, folder: path, name: review?.name ?? baseName(path), includeSubfolders, subfolderCount: 0, fileKinds, leftOut: 0, timed: false },
        loading: true,
        openError: null,
        photos: [],
        order: [],
        view: [],
        current: -1,
        selected: new Set(),
        metaProgress: 0,
        undoStack: [],
        redoStack: [],
        viewMode: openingView(get().defaultViewMode, get().rememberViewMode, get().viewMode),
        flagFilter: "all",
        ratingFilter: null,
        colorFilters: new Set(),
      });

      let reply;
      try {
        reply = await api.openFolder(path, includeSubfolders, fileKinds, review?.items.filter((p) => !get().ignoreVideos || !isVideo(p.path)), get().ignoreVideos);
      } catch (e) {
        set({ session: null, loading: false, openError: String(e) });
        return;
      }
      // Closed, or another folder opened, while this one was being listed.
      if (get().session?.folder !== path) {
        void api.closeFolder(reply.session);
        return;
      }

      const photos: Photo[] = reply.images.map((im, index) => ({
        ...im,
        index,
        rating: 0,
        flag: "none",
        color: "none",
        exif: null,
        captureMs: reply.times[index] ?? null,
        fullWidth: 0,
        fullHeight: 0,
        framing: PLAIN,
        framable: false,
        group: reply.groups?.[index] ?? null,
        ...review?.decisions[im.path],
      }));
      // When every capture time came back with the listing, the folder opens
      // in the order it was shot and on its true first frame. That matters
      // most with two cameras: each numbers its own frames, so by filename
      // one camera's whole day comes before the other's.
      //
      // When it did not — a slow card, a very large folder — it opens by
      // filename rather than keep anyone waiting, and settles into shooting
      // order once the times are in.
      const s0 = get();
      const order = reply.timed ? sorted(photos, s0.sort, s0.sortAscending) : photos.map((p) => p.index);

      // Pick up where this folder was left.
      const remembered = load<string | null>(`position.${path}`, null);
      // Where this folder was left. By the frame, if not by the file: left
      // on a RAW and opened for JPEGs alone, it is the same photo.
      const frame = (path: string) => path.replace(/\.[^./\\]+$/, "").toLowerCase();
      const exact = remembered ? photos.findIndex((p) => p.path === remembered) : -1;
      const resume = exact >= 0 || !remembered ? exact : photos.findIndex((p) => frame(p.path) === frame(remembered));
      const current = photos.length === 0 ? -1 : resume >= 0 ? resume : order[0];

      const recents = review ? get().recents : [path, ...get().recents.filter((r) => r !== path)].slice(0, MAX_RECENTS);
      if (!review) save("lastFolderSession", { path, updatedAt: Date.now() });
      set({
        session: { id: reply.session, folder: path, name: review?.name ?? baseName(path), includeSubfolders, subfolderCount: reply.subfolderCount, fileKinds, leftOut: reply.leftOut, timed: reply.timed },
        loading: false,
        photos,
        order,
        view: order,
        current,
        anchor: Math.max(0, current),
        recents,
        metaProgress: photos.length === 0 ? 1 : 0,
        ai: [],
        aiGroupingBusy: 0, aiGroupingError: null,
        compared: [],
        ranks: [],
        poses: [],
        aiDone: 0,
        motionBlurFilter: false, motionRatingSkipped: new Set(),
        aiCached: 0, aiPending: null,
        aiRunning: false,
        faceZoom: null,
        hoveredFace: null,
      });
      if (photos.length === 0) return;

      const sessionId = reply.session;
      let read = 0;
      const onEvent = (event: MetaEvent) => {
        const s = get();
        if (s.session?.id !== sessionId) return; // a folder since closed

        if (event.type === "batch") {
          const next = s.photos.slice();
          for (const m of event.items) {
            next[m.index] = { ...next[m.index], ...m.marks, exif: m.exif, captureMs: m.exif.captureMs ?? next[m.index].captureMs, fullWidth: m.fullWidth, fullHeight: m.fullHeight, framing: isPlain(m.framing) ? PLAIN : m.framing, framable: m.framable, ...(get().review?.decisions[next[m.index].path] ?? {}) };
          }
          read += event.items.length;
          set({ photos: next, view: filtered(s.order, next, s), metaProgress: read / next.length });
          return;
        }

        // Everything is read: bursts are known, and so are capture times. A
        // folder that could not be timed as it opened settles into the order
        // it was shot in now; one that could was already in it, and sorting
        // again changes nothing.
        const next = s.photos.map((p, i) => ({ ...p, group: event.groups[i] ?? null }));
        const order = sorted(next, s.sort, s.sortAscending);
        // If the folder opened by filename and nothing has been touched yet,
        // move to the true first frame rather than stay on whichever file
        // happened to sort first by name.
        const untouched = !reply.timed && resume < 0 && s.current === 0 && s.undoStack.length === 0 && s.selected.size === 0;
        set({
          photos: next,
          order,
          view: filtered(order, next, s),
          metaProgress: 1,
          ...(untouched ? { current: order[0], anchor: order[0] } : {}),
        });
      };
      try {
        await api.loadMetadata(sessionId, onEvent);
      } catch (e) {
        if (get().session?.id === sessionId) set({ metaProgress: 1, openError: String(e) });
      }
    },

    closeFolder() {
      const { session } = get();
      if (!session) return;
      rememberPosition();
      if (session.id >= 0) void api.closeFolder(session.id);
      set({ session: null, review: null, loading: false, photos: [], order: [], view: [], current: -1, selected: new Set(), trashPending: null, cropOpen: false, warmRemaining: 0, aiGroupingBusy: 0, aiGroupingError: null, ai: [], compared: [], ranks: [], poses: [], aiDone: 0, aiCached: 0, aiPending: null, aiRunning: false, faceZoom: null, hoveredFace: null });
    },

    reloadForSubfolders() {
      const folder = get().session?.folder;
      if (folder) void get().openFolder(folder, get().review ?? undefined);
    },

    select(index, mods) {
      const s = get();
      if (index < 0 || index >= s.photos.length) return;

      if (mods.meta) {
        // The first modified click starts a selection that already includes
        // the photo being looked at, as a file browser's does.
        const selected = new Set(s.selected);
        if (selected.size === 0 && s.current >= 0) selected.add(s.current);
        if (selected.has(index)) selected.delete(index);
        else selected.add(index);
        set({ selected });
      } else if (mods.shift) {
        const selected = new Set(s.selected);
        if (selected.size === 0 && s.current >= 0) selected.add(s.current);
        // A range is a run of what is on screen, so it is measured in the
        // order shown, not the order in the folder.
        const a = s.view.indexOf(s.anchor);
        const b = s.view.indexOf(index);
        if (a >= 0 && b >= 0) {
          for (let i = Math.min(a, b); i <= Math.max(a, b); i++) selected.add(s.view[i]);
        } else {
          selected.add(index);
        }
        set({ selected });
      } else {
        set({ selected: new Set(), anchor: index, current: index });
      }
    },

    selectAll() {
      const { view } = get();
      if (view.length > 0) set({ selected: new Set(view) });
    },

    go(index) {
      if (index >= 0 && index !== get().current) set({ current: index });
    },
    next: () => get().go(neighbour(1)),
    previous: () => get().go(neighbour(-1)),

    nextGroup() {
      const s = get();
      const { view, current } = s;
      const at = view.indexOf(current);
      if (at < 0) return get().next();
      const group = groupOf(s, current);
      for (let i = at + 1; i < view.length; i++) {
        if (group === null || groupOf(s, view[i]) !== group) return get().go(view[i]);
      }
      get().go(view[view.length - 1]);
    },

    previousGroup() {
      const s = get();
      const { view, current } = s;
      let at = view.indexOf(current);
      if (at <= 0) return get().previous();
      // Back out of this group, then to the first frame of the one before.
      const group = groupOf(s, current);
      at -= 1;
      while (at > 0 && group !== null && groupOf(s, view[at]) === group) at -= 1;
      const previous = groupOf(s, view[at]);
      while (at > 0 && previous !== null && groupOf(s, view[at - 1]) === previous) at -= 1;
      get().go(view[at]);
    },

    setRating(rating, targets) {
      const s = get();
      set({ motionRatingSkipped: new Set([...s.motionRatingSkipped, ...(targets ?? currentOnly())]) });
      const marked = targets === undefined;
      applyMarks((targets ?? currentOnly()).map((index) => ({ index, after: { ...marksOf(s.photos[index]), rating } })));
      if (marked && speedCulling(s)) get().next();
    },

    setFlag(flag, targets) {
      const s = get();
      const marked = targets === undefined;
      applyMarks((targets ?? currentOnly()).map((index) => ({ index, after: { ...marksOf(s.photos[index]), flag } })));
      if (marked && speedCulling(s) && flag !== "none") get().next();
    },

    setColor(color, targets) {
      const s = get();
      const list = targets ?? currentOnly();
      // Pressing a photo's own colour again takes it off. Only for a single
      // photo: across several it would clear some and set others.
      const value: Color = list.length === 1 && s.photos[list[0]].color === color ? "none" : color;
      applyMarks(list.map((index) => ({ index, after: { ...marksOf(s.photos[index]), color: value } })));
    },

    clearRatingAndColor(targets) {
      const s = get();
      const list = targets ?? (s.selected.size > 0 ? [...s.selected] : currentOnly());
      set({ motionRatingSkipped: new Set([...s.motionRatingSkipped, ...list]) });
      applyMarks(list.map((index) => ({ index, after: { ...marksOf(s.photos[index]), rating: 0, color: "none" as Color } })));
    },

    setFraming(index, framing) {
      applyFraming([{ index, framing }]);
    },

    turnPhotos(how, targets) {
      const s = get();
      const by = { left: QUARTER_LEFT, right: QUARTER_RIGHT, across: MIRROR_ACROSS, down: MIRROR_DOWN }[how];
      const list = targets ?? (s.selected.size > 0 ? [...s.selected] : currentOnly());
      applyFraming(list.map((index) => ({ index, framing: turned(s.photos[index], by) })));
    },

    resetFraming(targets) {
      const s = get();
      const list = targets ?? (s.selected.size > 0 ? [...s.selected] : currentOnly());
      applyFraming(list.map((index) => ({ index, framing: PLAIN })));
    },

    copyCrop(index) {
      const s = get();
      // While a crop is being made, it is the crop as it stands that is copied.
      const making = useCrop.getState();
      if (s.cropOpen && making.index !== null && (index === undefined || index === making.index)) {
        const made = making.result().crop;
        set({ copiedCrop: made ? shareOf(made, making.view) : null });
        return;
      }
      const photo = s.photos[index ?? s.current];
      // A photo with no crop has none to copy, and leaves nothing behind to
      // be pasted by mistake.
      set({ copiedCrop: photo?.framing.crop ? shareOf(photo.framing.crop, viewSize(photo)) : null });
    },

    pasteCrop(targets) {
      const s = get();
      const share = s.copiedCrop;
      if (!share) return;
      const list = targets ?? (s.selected.size > 0 ? [...s.selected] : currentOnly());
      // The same share of each photo, whichever way up it has been turned.
      applyFraming(list.flatMap((index) => (s.photos[index] ? [{ index, framing: { turn: s.photos[index].framing.turn, crop: cropFromShare(share, viewSize(s.photos[index])) } }] : [])));
    },

    undo() {
      const s = get();
      const edit = s.undoStack[s.undoStack.length - 1];
      if (!edit) return;
      set({ motionRatingSkipped: new Set([...s.motionRatingSkipped, ...edit.map((e) => e.index)]) });
      applyMarks(edit.filter((e) => !e.framing).map((e) => ({ index: e.index, after: e.before })), false);
      applyFraming(edit.flatMap((e) => (e.framing ? [{ index: e.index, framing: e.framing.before }] : [])), false);
      set({ undoStack: s.undoStack.slice(0, -1), redoStack: [...s.redoStack, edit], ...backTo(edit) });
    },

    redo() {
      const s = get();
      const edit = s.redoStack[s.redoStack.length - 1];
      if (!edit) return;
      set({ motionRatingSkipped: new Set([...s.motionRatingSkipped, ...edit.map((e) => e.index)]) });
      applyMarks(edit.filter((e) => !e.framing).map((e) => ({ index: e.index, after: e.after })), false);
      applyFraming(edit.flatMap((e) => (e.framing ? [{ index: e.index, framing: e.framing.after }] : [])), false);
      set({ redoStack: s.redoStack.slice(0, -1), undoStack: [...s.undoStack, edit], ...backTo(edit) });
    },

    setFlagFilter(flagFilter) {
      const s = get();
      refilter({ flagFilter }, filtered(s.order, s.photos, { ...s, flagFilter }));
    },
    setRatingFilter(ratingFilter) {
      const s = get();
      refilter({ ratingFilter }, filtered(s.order, s.photos, { ...s, ratingFilter }));
    },
    setMotionBlurFilter(motionBlurFilter) {
      const s = get();
      refilter({ motionBlurFilter }, filtered(s.order, s.photos, { ...s, motionBlurFilter }));
    },
    rateMotionBlur() {
      const s = get();
      if (!s.aiEnabled || !s.aiMotionBlur || !s.autoRateMotionBlur || s.selfTest || s.metaProgress < 1) return;
      const changes = s.photos.flatMap((p) => p.rating === 0 && p.flag !== "pick" && !s.motionRatingSkipped.has(p.index) && hasMotionBlur(s.ai[p.index])
        ? [{ index: p.index, after: { ...marksOf(p), rating: 1, flag: "reject" as const } }] : []);
      applyMarks(changes);
    },
    toggleColorFilter(color) {
      const s = get();
      const colorFilters = new Set(s.colorFilters);
      if (colorFilters.has(color)) colorFilters.delete(color);
      else colorFilters.add(color);
      refilter({ colorFilters }, filtered(s.order, s.photos, { ...s, colorFilters }));
    },
    clearFilters() {
      const s = get();
      set({ flagFilter: "all", ratingFilter: null, motionBlurFilter: false, colorFilters: new Set(), view: s.order });
    },

    setSort(sort, sortAscending) {
      const s = get();
      const order = sorted(s.photos, sort, sortAscending);
      set({ sort, sortAscending, order, view: filtered(order, s.photos, s) });
    },

    set: (key, value) => set({ [key]: value } as Pick<State, typeof key>),
    toggle: (key) => set((s) => ({ [key]: !s[key] }) as Pick<State, typeof key>),
    zoomCommand: (kind) => set((s) => ({ zoom: { kind, nonce: s.zoom.nonce + 1 } })),

    regroup() {
      const { ai, session, smartGroups, groupingRevision, aiMobileClipGrouping, aiEnabled, mobileClipGrouping } = get();
      const revision = groupingRevision + 1;
      set({ compared: [], ranks: [], poses: [], groupingRevision: revision, aiGroupingBusy: 0, aiGroupingError: null });
      if (session) void api.aiCompare([], smartGroups, { session: session.id, revision, mobileClip: aiMobileClipGrouping && aiEnabled, mobileClipGrouping }).catch(() => {});
      compareRuns(ai.flatMap((result, index) => (result === undefined ? [] : [index])));
    },

    addAnalysis(items) {
      const s = get();
      const ai = s.ai.slice();
      let fresh = 0;
      for (const { index, ...result } of items) {
        if (ai[index] === undefined) fresh += 1;
        ai[index] = result;
      }
      set({ ai, aiDone: s.aiDone + fresh, view: filtered(s.order, s.photos, { ...s, ai }) });
      get().rateMotionBlur();
      compareRuns(items.map((item) => item.index));
    },

    zoomToFace: (face) => set((s) => ({ faceZoom: { x: face.x + face.w / 2, y: face.y + face.h / 2, nonce: (s.faceZoom?.nonce ?? 0) + 1 } })),

    requestTrash(targets) {
      const s = get();
      const list = targets ?? (s.selected.size > 0 ? [...s.selected] : currentOnly());
      if (list.length === 0) return;
      if (s.review || s.askBeforeTrash) set({ trashPending: list });
      else void trash(list);
    },

    async confirmTrash() {
      const pending = get().trashPending;
      if (!pending) return;
      set({ trashPending: null });
      await trash(pending);
    },

    async transfer(targets, moving) {
      const s = get();
      if (s.review) { set({ notice: { title: "Finish card review to import", body: "Use Finish review to copy selected files with their local marks, or import them in place. Sources are not moved during card review." } }); return; }
      if (!s.session || targets.length === 0) return;
      const count = `${targets.length} photo${targets.length === 1 ? "" : "s"}`;
      const destination = await api.pickFolder(moving ? `Move ${count} to:` : `Copy ${count} to:`);
      if (!destination) return;

      const session = s.session.id;
      const items = targets.map((i) => ({ path: s.photos[i].path, display: s.photos[i].display }));
      let result;
      try {
        result = await api.transferPhotos(items, destination, moving);
      } catch (e) {
        set({ notice: { title: moving ? "The photos were not moved" : "The photos were not copied", body: String(e) } });
        return;
      }
      // The folder may have been closed, or another opened, while this ran.
      if (get().session?.id === session) get().forget(result.moved);

      const problems = [...result.failed, ...result.sidecarFailed.map((f) => `${f} (its ratings did not follow)`)];
      if (problems.length > 0) {
        const shown = problems.slice(0, 5).join("\n");
        const more = problems.length > 5 ? `\n…and ${problems.length - 5} more.` : "";
        set({ notice: { title: moving ? "Some photos were not moved" : "Some photos were not copied", body: `${shown}${more}\n\nNothing at the destination was replaced.` } });
      }
    },

    openCrop(index) {
      const s = get();
      if (!s.session) return;
      const at = index ?? s.current;
      const photo = s.photos[at];
      if (!photo) return;
      if (!photo.framable || photo.fullWidth <= 0) {
        set({
          notice: {
            title: "This photo cannot be cropped here",
            body: "Either its sidecar file could not be read with certainty, or the picture shown for it is not the whole of the photo, as when a camera is set to a different shape from its sensor.\n\nRather than risk a crop landing in the wrong place, or anything in the sidecar being lost, the photo is left exactly as it is.",
          },
        });
        return;
      }
      // Cropping is done on the big photo, so that is where it goes.
      set({ cropOpen: true, ...(s.viewMode === "grid" ? { viewMode: "loupe" as ViewMode } : {}), ...(at !== s.current ? { current: at, anchor: at, selected: new Set<number>() } : {}) });
    },

    acceptCrop() {
      const crop = useCrop.getState();
      if (get().cropOpen && crop.index !== null) applyFraming([{ index: crop.index, framing: crop.result() }]);
    },

    closeCrop(accept) {
      if (accept) get().acceptCrop();
      useCrop.getState().end();
      set({ cropOpen: false });
    },

    openShip(targets) {
      if (get().review) { set({ notice: { title: "Finish card review to import", body: "Use Finish review to choose which card files to import." } }); return; }
      if (!get().session) return;
      // Ship works from the selection, so shipping these means selecting them.
      if (targets && targets.length > 0) set({ selected: new Set(targets) });
      set({ shipOpen: true });
    },

    forget(paths) {
      const gone = new Set(paths);
      const { photos, order } = get();
      drop(new Set(order.filter((i) => gone.has(photos[i].path))));
      const review = get().review;
      if (review) set({ review: { ...review, items: review.items.filter((p) => !gone.has(p.path)), chosen: review.chosen?.filter((path) => !gone.has(path)) ?? null } });
    },

    removeRecents: (paths) => set((s) => ({ recents: s.recents.filter((r) => !paths.includes(r)) })),
    relocateRecent: (from, to) => set((s) => ({ recents: s.recents.map((r) => (r === from ? to : r)).filter((r, i, all) => all.indexOf(r) === i) })),
  };
});

// Settings are written back as they change, each only when it actually has.
let last = useStore.getState();
useStore.subscribe((state) => {
  for (const key of PERSISTED) {
    if (state[key] !== last[key]) save(key, state[key]);
  }
  if (state.review && (state.photos !== last.photos || state.review !== last.review) && !state.loading) {
    void saveReview(state.review).catch((error) => useStore.setState({ notice: { title: "Card session could not be saved", body: String(error) } }));
  }
  last = state;
});

// Quitting from the culling view is the normal way to stop for the night, so
// the place is recorded then, and not only on the way back to the home screen.
window.addEventListener("beforeunload", () => {
  const { session, photos, current } = useStore.getState();
  if (session && current >= 0) save(`position.${session.folder}`, photos[current].path);
});

/** The photos a right-click acts on: the selection if it holds the photo. */
export function contextTargets(index: number): number[] {
  const { selected } = useStore.getState();
  return selected.has(index) && selected.size > 1 ? [...selected] : [index];
}
