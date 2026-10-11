// The state of the app: which folder is open, what has been decided about
// each photo in it, and what is on screen.

import { create } from "zustand";
import { api, describeFailure, logged } from "./api";
import { load, save } from "./prefs";
import { baseName, filtered, groupOf, marksOf, rankRun, runsOf, sorted, uprightOf } from "./cull-logic";

// What views take from the culling rules, from where they always have.
export { BLINK, groupOf, groupsOf, hasFilter as hasActiveFilter, PROBLEM_LABEL, problemOf, type Problem } from "./cull-logic";
import { FILE_KINDS, SORT_KEYS, type AiFace, type AiResult, type Color, type Compared, type FileKinds, type Flag, type FlagFilter, type Marks, type MetaEvent, type Photo, type Rank, type SortKey, type ViewMode } from "./types";

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
type Edit = { index: number; before: Marks; after: Marks }[];

export type ZoomCommand = { kind: "toggle" | "centre" | "in" | "out"; nonce: number };

/** The keys of `T` that hold values rather than actions. */
type ValueKeys<T> = { [K in keyof T]: T[K] extends (...args: never[]) => unknown ? never : K }[keyof T];

/**
 * The folder's own data. Only the store's actions change these, since each
 * change has to keep the others in step: `view` is drawn from `order`, which
 * indexes `photos`, which the AI arrays parallel.
 */
type FolderData = "session" | "photos" | "order" | "view" | "current" | "selected" | "undoStack" | "redoStack" | "ai" | "compared" | "ranks" | "poses" | "aiDone" | "aiRescan";

/** What `set` may change: settings and what is showing. */
export type Settable = Exclude<ValueKeys<State>, FolderData>;

interface State {
  platform: string;
  selfTest: boolean;

  session: Session | null;
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
  colorFilters: Set<Color>;
  sort: SortKey;
  sortAscending: boolean;

  viewMode: ViewMode;
  gridColumns: number;
  showExif: boolean;
  showFilmstrip: boolean;
  showInfo: boolean;
  showRatingBar: boolean;
  showFilterBar: boolean;
  filmstripHeight: number;
  /** The strip's edge is being dragged, and its height is changing every frame. */
  stripResizing: boolean;
  includeSubfolders: boolean;
  /** Which of a folder's files are shown as its photos. */
  fileKinds: FileKinds;
  /** Write a JPEG's marks into the file itself, as well as beside it. */
  embedInJpeg: boolean;
  /** Move on to the next photo after marking one. Follows Caps Lock. */
  speedCull: boolean;
  /** The same, left on whatever Caps Lock is doing. */
  speedCullAlways: boolean;
  /** Carry the zoom, and the spot looked at, from one photo to the next. */
  keepZoom: boolean;
  /** Fetch the full-size picture of a photo rested on, so zooming is immediate. */
  instantZoom: boolean;
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
  /** How many photos have been analysed, and whether analysis is still going. */
  aiDone: number;
  aiRunning: boolean;
  /** Bumped to start the folder's face scan again from nothing. */
  aiRescan: number;
  /** Find and judge faces at all. */
  aiEnabled: boolean;
  /** Show faces judged to be background in the face panel, dimmed. */
  showBackgroundFaces: boolean;
  /** Analysis stands aside whenever a photo is being moved to. */
  aiPause: boolean;
  /** The panel of faces beside the photo. */
  showFaces: boolean;
  /** A face to zoom the loupe to; numbered so the same one can be asked for twice. */
  faceZoom: { x: number; y: number; nonce: number } | null;

  shortcutsOpen: boolean;
  settingsOpen: boolean;
  shipOpen: boolean;
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
  openFolder(path: string): Promise<void>;
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
  undo(): void;
  redo(): void;

  setFlagFilter(f: FlagFilter): void;
  setRatingFilter(r: number | null): void;
  toggleColorFilter(c: Color): void;
  clearFilters(): void;
  setSort(key: SortKey, ascending: boolean): void;
  /**
   * Puts the folder back in sorted order. Marking a photo does not move it,
   * even when sorting by rating — a photo jumping away the moment it is rated
   * loses the photographer's place — so the order is brought up to date only
   * when asked.
   */
  resort(): void;

  set<K extends Settable>(key: K, value: State[K]): void;
  toggle(key: "showExif" | "showFilmstrip" | "showInfo" | "showRatingBar" | "showFilterBar" | "showFaces"): void;
  /** Compares every analysed photo afresh, after a change to what counts as a group. */
  regroup(): void;
  /** Forgets what analysis found for the folder on screen and starts it again. */
  rescanFaces(): void;
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
  /** Takes photos that have left the folder out of what is shown. */
  forget(paths: string[]): void;

  removeRecents(paths: string[]): void;
  relocateRecent(from: string, to: string): void;
}

export type Store = State & Actions;

export const GIGABYTE = 1024 ** 3;

/** Whether marking a photo should move on to the next one. */
export const speedCulling = (s: Pick<State, "speedCull" | "speedCullAlways">) => s.speedCull || s.speedCullAlways;

/** Settings written back to storage whenever they change. */
const PERSISTED = [
  "sort",
  "sortAscending",
  "gridColumns",
  "showExif",
  "showFilmstrip",
  "showInfo",
  "showRatingBar",
  "showFilterBar",
  "filmstripHeight",
  "includeSubfolders",
  "fileKinds",
  "embedInJpeg",
  "warmAhead",
  "speedCullAlways",
  "keepZoom",
  "instantZoom",
  "rgbHistogram",
  "cacheLimitGb",
  "aiEnabled",
  "showBackgroundFaces",
  "aiPause",
  "smartGroups",
  "showFaces",
  "recents",
] as const satisfies readonly (keyof State)[];

const MAX_RECENTS = 24;
const MAX_UNDO = 200;

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
    void api.setMarks(edit.map((e) => ({ path: photos[e.index].path, into: jpegs(photos[e.index]), ...e.after }))).catch(logged("saving marks"));
    set({
      photos,
      // A photo that no longer passes the filter leaves the view at once,
      // which is what filtering to "unrated" and rating your way through
      // the folder relies on.
      view: filtered(s.order, photos, s),
      ...(record ? { undoStack: [...s.undoStack.slice(-MAX_UNDO + 1), edit], redoStack: [] } : {}),
    });
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

  // Counts folder openings and closings, so a listing that arrives after
  // another opening or a close is known to be stale.
  let openings = 0;

  /**
   * Compares the frames of every run one of these photos belongs to, if all
   * of the run is now analysed: the same people set beside themselves, in a
   * burst or in a pose shot again. Not before then — a rank that changes as
   * the scan reaches the rest would have the photographer picking "the
   * best" of three frames out of seven.
   */
  function compareRuns(touched: number[]) {
    const { photos, ai, session, smartGroups } = get();
    if (!session) return;
    if (runs?.session !== session.id) {
      const list = runsOf(photos).filter((run) => run.length >= 2);
      const of = new Map<number, number>();
      list.forEach((run, at) => run.forEach((index) => of.set(index, at)));
      runs = { session: session.id, list, of };
    }
    const { list, of } = runs;
    const ready = [...new Set(touched.map((i) => of.get(i)).filter((at) => at !== undefined))]
      .map((at) => list[at])
      .filter((run) => run.every((i) => ai[i] !== undefined))
      // A photo that could not be analysed takes no part: it has nothing to
      // be set beside the others with.
      .map((run) => run.filter((i) => !ai[i]!.failed))
      .filter((run) => run.length >= 2);
    if (ready.length === 0) return;
    void api
      .aiCompare(ready.map((run) => run.map((i) => ({ ...ai[i]!, time: photos[i].captureMs, burst: photos[i].group, upright: uprightOf(photos[i]) }))), smartGroups)
      .then((marked) => {
        const now = get();
        // A folder since closed, or the setting changed while this was asked.
        if (now.session?.id !== session.id || now.smartGroups !== smartGroups) return;
        const compared = now.compared.slice();
        const ranks = now.ranks.slice();
        const poses = now.poses.slice();
        ready.forEach((run, r) => {
          run.forEach((index, at) => (compared[index] = marked[r][at]));
          rankRun(ranks, run, marked[r]);
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
        set({ compared, ranks, poses });
      })
      .catch(logged("comparing frames"));
  }

  function rememberPosition() {
    const { session, photos, current } = get();
    if (session && current >= 0) save(`position.${session.folder}`, photos[current].path);
  }

  return {
    platform: "",
    selfTest: false,
    session: null,
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
    colorFilters: new Set(),
    sort: load<SortKey>("sort", "captureTime", SORT_KEYS),
    sortAscending: load("sortAscending", true),

    viewMode: "loupe",
    gridColumns: load("gridColumns", 4),
    showExif: load("showExif", false),
    showFilmstrip: load("showFilmstrip", true),
    showInfo: load("showInfo", false),
    showRatingBar: load("showRatingBar", true),
    showFilterBar: load("showFilterBar", false),
    filmstripHeight: load("filmstripHeight", 120),
    stripResizing: false,
    includeSubfolders: load("includeSubfolders", false),
    fileKinds: load<FileKinds>("fileKinds", "both", FILE_KINDS),
    // Off unless asked for: rating a photo should not rewrite the original
    // file unless the photographer chose that.
    embedInJpeg: load("embedInJpeg", false),
    speedCull: false,
    speedCullAlways: load("speedCullAlways", false),
    keepZoom: load("keepZoom", true),
    instantZoom: load("instantZoom", true),
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
    aiDone: 0,
    aiRunning: false,
    aiRescan: 0,
    aiEnabled: load("aiEnabled", true),
    showBackgroundFaces: load("showBackgroundFaces", false),
    aiPause: load("aiPause", true),
    showFaces: load("showFaces", true),
    faceZoom: null,

    shortcutsOpen: false,
    settingsOpen: false,
    shipOpen: false,
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
      set({ platform: info.platform, selfTest: info.selfTest });
      // A self-test's cache is its own and thrown away; it has no limit to keep.
      if (!info.selfTest) void api.setCacheLimit(get().cacheLimitGb * GIGABYTE).catch(logged("setting the preview cache limit"));
      if (info.selfTest && info.openPath) {
        const { runSelfTest } = await import("./selftest");
        void runSelfTest(info.openPath);
      }
      if (info.openPath) await get().openFolder(info.openPath);
    },

    async openFolder(path) {
      const before = get();
      if (before.session) get().closeFolder();
      // Which opening this is. The same folder opened twice before the first
      // reply arrives must not keep both: the first backend session would be
      // left open, still reading the folder, with nothing to close it.
      const mine = ++openings;

      const { includeSubfolders, fileKinds } = get();
      set({
        session: { id: -1, folder: path, name: baseName(path), includeSubfolders, subfolderCount: 0, fileKinds, leftOut: 0, timed: false },
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
        viewMode: "loupe",
        flagFilter: "all",
        ratingFilter: null,
        colorFilters: new Set(),
      });

      let reply;
      try {
        reply = await api.openFolder(path, includeSubfolders, fileKinds);
      } catch (e) {
        set({ session: null, loading: false, openError: String(e) });
        return;
      }
      // Closed, or another folder (or this one again) opened, while this
      // one was being listed.
      if (mine !== openings || get().session?.folder !== path) {
        void api.closeFolder(reply.session).catch(logged("closing a folder"));
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
        group: reply.groups?.[index] ?? null,
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

      const recents = [path, ...get().recents.filter((r) => r !== path)].slice(0, MAX_RECENTS);
      set({
        session: { id: reply.session, folder: path, name: baseName(path), includeSubfolders, subfolderCount: reply.subfolderCount, fileKinds, leftOut: reply.leftOut, timed: reply.timed },
        loading: false,
        photos,
        order,
        view: order,
        current,
        anchor: Math.max(0, current),
        recents,
        metaProgress: photos.length === 0 ? 1 : 0,
        ai: [],
        compared: [],
        ranks: [],
        poses: [],
        aiDone: 0,
        aiRunning: false,
        faceZoom: null,
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
            next[m.index] = { ...next[m.index], ...m.marks, exif: m.exif, captureMs: m.exif.captureMs ?? next[m.index].captureMs, fullWidth: m.fullWidth, fullHeight: m.fullHeight };
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
      openings += 1;
      rememberPosition();
      if (session.id >= 0) void api.closeFolder(session.id).catch(logged("closing a folder"));
      set({ session: null, loading: false, photos: [], order: [], view: [], current: -1, selected: new Set(), trashPending: null, warmRemaining: 0, ai: [], compared: [], ranks: [], poses: [], aiDone: 0, aiRunning: false, faceZoom: null });
    },

    reloadForSubfolders() {
      const folder = get().session?.folder;
      if (folder) void get().openFolder(folder);
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
      applyMarks(list.map((index) => ({ index, after: { ...marksOf(s.photos[index]), rating: 0, color: "none" as Color } })));
    },

    undo() {
      const s = get();
      const edit = s.undoStack[s.undoStack.length - 1];
      if (!edit) return;
      applyMarks(edit.map((e) => ({ index: e.index, after: e.before })), false);
      set({ undoStack: s.undoStack.slice(0, -1), redoStack: [...s.redoStack, edit], ...backTo(edit) });
    },

    redo() {
      const s = get();
      const edit = s.redoStack[s.redoStack.length - 1];
      if (!edit) return;
      applyMarks(edit.map((e) => ({ index: e.index, after: e.after })), false);
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
    toggleColorFilter(color) {
      const s = get();
      const colorFilters = new Set(s.colorFilters);
      if (colorFilters.has(color)) colorFilters.delete(color);
      else colorFilters.add(color);
      refilter({ colorFilters }, filtered(s.order, s.photos, { ...s, colorFilters }));
    },
    clearFilters() {
      const s = get();
      set({ flagFilter: "all", ratingFilter: null, colorFilters: new Set(), view: s.order });
    },

    setSort(sort, sortAscending) {
      const s = get();
      const order = sorted(s.photos, sort, sortAscending);
      set({ sort, sortAscending, order, view: filtered(order, s.photos, s) });
    },

    resort() {
      const { sort, sortAscending } = get();
      get().setSort(sort, sortAscending);
    },

    set: (key, value) => set({ [key]: value } as Pick<State, typeof key>),
    toggle: (key) => set((s) => ({ [key]: !s[key] }) as Pick<State, typeof key>),
    zoomCommand: (kind) => set((s) => ({ zoom: { kind, nonce: s.zoom.nonce + 1 } })),

    rescanFaces: () => set((s) => ({ ai: [], compared: [], ranks: [], poses: [], aiDone: 0, aiRescan: s.aiRescan + 1 })),

    regroup() {
      const { ai } = get();
      set({ compared: [], ranks: [], poses: [] });
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
      set({ ai, aiDone: s.aiDone + fresh });
      compareRuns(items.map((item) => item.index));
    },

    zoomToFace: (face) => set((s) => ({ faceZoom: { x: face.x + face.w / 2, y: face.y + face.h / 2, nonce: (s.faceZoom?.nonce ?? 0) + 1 } })),

    requestTrash(targets) {
      const s = get();
      const list = targets ?? (s.selected.size > 0 ? [...s.selected] : currentOnly());
      if (list.length > 0) set({ trashPending: list });
    },

    async confirmTrash() {
      const s = get();
      const pending = s.trashPending;
      if (!pending || !s.session) return;
      set({ trashPending: null });

      const session = s.session.id;
      const items = pending.map((i) => ({ path: s.photos[i].path, display: s.photos[i].display }));
      let failures;
      try {
        failures = await api.moveToTrash(items);
      } catch (e) {
        set({ notice: { title: "Nothing was moved to the Trash", body: String(e) } });
        return;
      }
      // The folder may have been closed, or another opened, while this ran.
      // Its photos are no longer the ones `pending` points at.
      if (get().session?.id !== session) return;
      const failed = new Set(failures.map((f) => f.path));
      drop(new Set(pending.filter((i) => !failed.has(s.photos[i].path))));
      if (failures.length > 0) {
        const name = (path: string) => path.split(/[\\/]/).pop() ?? path;
        const shown = failures.slice(0, 5).map((f) => `${name(f.path)}: ${f.reason}`).join("\n");
        const more = failures.length > 5 ? `\n…and ${failures.length - 5} more.` : "";
        set({ notice: { title: `${failures.length} photo${failures.length === 1 ? " was" : "s were"} not moved to the Trash`, body: `${shown}${more}\n\nThey are still in the folder.` } });
      }
    },

    async transfer(targets, moving) {
      const s = get();
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

      const problems = [...result.failed.map(describeFailure), ...result.sidecarFailed.map((f) => `${describeFailure(f)} (its ratings did not follow)`)];
      if (problems.length > 0) {
        const shown = problems.slice(0, 5).join("\n");
        const more = problems.length > 5 ? `\n…and ${problems.length - 5} more.` : "";
        set({ notice: { title: moving ? "Some photos were not moved" : "Some photos were not copied", body: `${shown}${more}\n\nNothing at the destination was replaced.` } });
      }
    },

    openShip(targets) {
      if (!get().session) return;
      // Ship works from the selection, so shipping these means selecting them.
      if (targets && targets.length > 0) set({ selected: new Set(targets) });
      set({ shipOpen: true });
    },

    forget(paths) {
      const gone = new Set(paths);
      const { photos, order } = get();
      drop(new Set(order.filter((i) => gone.has(photos[i].path))));
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
