import { beforeEach, describe, expect, it, vi } from "vitest";

import { api } from "./api";
import { PLAIN } from "./framing";
import { useStore } from "./store";
import type { AiResult, Compared, Photo } from "./types";

function photo(index: number, rating = 0, flag: Photo["flag"] = "none"): Photo {
  return {
    path: `/photos/${index}.jpg`,
    display: `/photos/${index}.jpg`,
    name: `${index}.jpg`,
    subpath: null,
    version: "v1",
    size: 1000,
    modified: 0,
    index,
    rating,
    flag,
    color: "none",
    exif: null,
    captureMs: index * 1000,
    fullWidth: 1200,
    fullHeight: 800,
    framing: PLAIN,
    framable: true,
    group: null,
  };
}

function result(): AiResult {
  return { faces: [], score: null, focus: null, gaze: null, key: null };
}

function resetStore(photos = [photo(0), photo(1), photo(2), photo(3)]) {
  useStore.setState({
    session: null,
    photos,
    order: photos.map((p) => p.index),
    view: photos.map((p) => p.index),
    current: 0,
    selected: new Set(),
    ai: [],
    aiDone: 0,
    aiEnabled: true,
    aiMobileClipGrouping: false,
    mobileClipGrouping: "normal",
    metaProgress: 1,
    selfTest: false,
    embedInJpeg: false,
    flagFilter: "all",
    ratingFilter: null,
    colorFilters: new Set(),
    undoStack: [],
    redoStack: [],
  });
}

describe("AI grouping controls", () => {
  beforeEach(() => {
    vi.restoreAllMocks();
    resetStore();
  });

  it("persists MobileCLIP preferences", () => {
    useStore.getState().set("aiMobileClipGrouping", true);
    useStore.getState().set("mobileClipGrouping", "tight");
    expect(localStorage.getItem("opencull.aiMobileClipGrouping")).toBe("true");
    expect(localStorage.getItem("opencull.mobileClipGrouping")).toBe('"tight"');
  });

  it("ignores grouping results after its session or settings revision changes", async () => {
    let resolveRevision!: (value: Compared[][]) => void;
    const revisionPending = new Promise<Compared[][]>((done) => { resolveRevision = done; });
    let resolveSession!: (value: Compared[][]) => void;
    const sessionPending = new Promise<Compared[][]>((done) => { resolveSession = done; });
    const compare = vi.spyOn(api, "aiCompare").mockReturnValueOnce(revisionPending).mockReturnValueOnce(sessionPending);
    const session = (id: number) => ({
      id,
      folder: `/folder-${id}`,
      name: `folder-${id}`,
      includeSubfolders: false,
      subfolderCount: 0,
      fileKinds: "both" as const,
      leftOut: 0,
      timed: false,
    });
    useStore.setState({
      session: session(1),
      metaProgress: 1,
      aiEnabled: true,
      smartGroups: true,
      photos: [photo(0), photo(1)],
      order: [0, 1],
      view: [0, 1],
      ai: [result(), result()],
      compared: [],
      ranks: [],
      poses: [],
    });

    useStore.getState().regroup();
    expect(api.aiCompare).toHaveBeenCalledOnce();
    useStore.setState({ groupingRevision: useStore.getState().groupingRevision + 1 });
    resolveRevision([[{ pose: 0, score: 100, eyes: [] }, { pose: 0, score: 0, eyes: [] }]]);
    await revisionPending;
    await Promise.resolve();
    expect(useStore.getState().compared).toEqual([]);

    useStore.setState({ groupingRevision: 0, compared: [], ranks: [], poses: [] });
    useStore.getState().regroup();
    expect(compare).toHaveBeenCalledTimes(2);
    useStore.setState({ session: session(2) });
    resolveSession([[{ pose: 0, score: 100, eyes: [] }, { pose: 0, score: 0, eyes: [] }]]);
    await sessionPending;
    await Promise.resolve();

    expect(useStore.getState().compared).toEqual([]);
    expect(useStore.getState().ranks).toEqual([]);
  });

  it("invalidates pending grouping synchronously before closing the session", async () => {
    let resolve!: (value: Compared[][]) => void;
    const pending = new Promise<Compared[][]>((done) => { resolve = done; });
    vi.spyOn(api, "aiCompare").mockReturnValueOnce(pending);
    const session = {
      id: 7,
      folder: "/folder-7",
      name: "folder-7",
      includeSubfolders: false,
      subfolderCount: 0,
      fileKinds: "both" as const,
      leftOut: 0,
      timed: false,
    };
    useStore.setState({
      session,
      metaProgress: 1,
      aiEnabled: true,
      aiMobileClipGrouping: true,
      smartGroups: true,
      photos: [photo(0), photo(1)],
      order: [0, 1],
      view: [0, 1],
      ai: [result(), result()],
    });
    useStore.getState().regroup();
    const revision = useStore.getState().groupingRevision;
    vi.spyOn(api, "closeFolder").mockImplementation(() => {
      expect(useStore.getState().groupingRevision).toBe(revision + 1);
      return Promise.resolve();
    });

    useStore.getState().closeFolder();
    expect(useStore.getState().session).toBeNull();
    expect(useStore.getState().groupingRevision).toBe(revision + 1);
    resolve([[{ pose: 0, score: 100, eyes: [] }, { pose: 0, score: 0, eyes: [] }]]);
    await pending;
    await Promise.resolve();
    expect(useStore.getState().compared).toEqual([]);
    expect(useStore.getState().ranks).toEqual([]);
  });
});
