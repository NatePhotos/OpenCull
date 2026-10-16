import { beforeEach, describe, expect, it, vi } from "vitest";

import { api } from "./api";
import { PLAIN } from "./framing";
import { hasMotionBlur } from "./motion-blur";
import { problemOf, useStore } from "./store";
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

function result(motionBlur: number | null): AiResult {
  return { faces: [], score: null, focus: null, gaze: null, key: null, motionBlur };
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
    aiMotionBlur: true,
    autoRateMotionBlur: false,
    motionBlurFilter: false,
    motionMarkOverrides: new Set(),
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

describe("AI grouping and motion-blur controls", () => {
  beforeEach(() => {
    vi.restoreAllMocks();
    resetStore();
  });

  it("persists MobileCLIP and motion-blur preferences", () => {
    useStore.getState().set("aiMobileClipGrouping", true);
    useStore.getState().set("mobileClipGrouping", "tight");
    useStore.getState().set("aiMotionBlur", true);
    expect(localStorage.getItem("opencull.aiMobileClipGrouping")).toBe("true");
    expect(localStorage.getItem("opencull.mobileClipGrouping")).toBe('"tight"');
    expect(localStorage.getItem("opencull.aiMotionBlur")).toBe("true");
  });

  it("auto marks detected photos while preserving existing ratings, picks, and manual decisions", () => {
    resetStore([photo(0), { ...photo(1, 4), color: "green" }, photo(2, 0, "pick"), photo(3)]);
    useStore.getState().setRating(0, [3]); // An explicit unrated choice also wins over automation.
    useStore.getState().set("autoRateMotionBlur", true);
    useStore.getState().addAnalysis([0, 1, 2, 3].map((index) => ({ index, ...result(0.9) })));

    expect(useStore.getState().photos.map(({ rating, flag }) => [rating, flag])).toEqual([
      [1, "reject"],
      [4, "reject"],
      [0, "pick"],
      [0, "none"],
    ]);
    expect(useStore.getState().photos[1].color).toBe("green");
    expect(useStore.getState().current).toBe(0);
    expect(useStore.getState().undoStack.at(-1)?.map(({ index }) => index)).toEqual([0, 1]);
  });

  it("does not auto-mark while automation is off or metadata is incomplete", () => {
    useStore.getState().addAnalysis([{ index: 0, ...result(0.95) }]);
    expect(useStore.getState().photos[0]).toMatchObject({ rating: 0, flag: "none" });

    resetStore([photo(0)]);
    useStore.getState().set("autoRateMotionBlur", true);
    useStore.setState({ metaProgress: 0 });
    useStore.getState().addAnalysis([{ index: 0, ...result(0.95) }]);
    expect(useStore.getState().photos[0]).toMatchObject({ rating: 0, flag: "none" });
  });

  it("does not auto-mark when motion-blur detection is disabled", () => {
    useStore.getState().set("autoRateMotionBlur", true);
    useStore.setState({ aiMotionBlur: false });
    useStore.getState().addAnalysis([{ index: 0, ...result(0.95) }]);
    expect(useStore.getState().photos[0]).toMatchObject({ rating: 0, flag: "none" });
  });

  it("does not auto-mark while OpenCull AI is disabled", () => {
    useStore.getState().set("autoRateMotionBlur", true);
    useStore.setState({ aiEnabled: false });
    useStore.getState().addAnalysis([{ index: 0, ...result(0.95) }]);
    expect(useStore.getState().photos[0]).toMatchObject({ rating: 0, flag: "none" });
  });

  it("undoes automatic marks and treats the undo as a manual override", () => {
    resetStore([photo(0)]);
    useStore.getState().set("autoRateMotionBlur", true);
    useStore.getState().addAnalysis([{ index: 0, ...result(0.95) }]);
    expect(useStore.getState().photos[0]).toMatchObject({ rating: 1, flag: "reject" });

    useStore.getState().undo();
    expect(useStore.getState().photos[0]).toMatchObject({ rating: 0, flag: "none" });
    useStore.getState().rateMotionBlur();
    expect(useStore.getState().photos[0]).toMatchObject({ rating: 0, flag: "none" });
    expect(useStore.getState().redoStack).toHaveLength(1);
  });

  it("filters motion-blurred photos, composes with other filters and clears cleanly", () => {
    resetStore([photo(0), photo(1), { ...photo(2, 4, "reject"), color: "yellow" }, photo(3)]);
    useStore.getState().set("autoRateMotionBlur", false);
    useStore.getState().addAnalysis([0, 1, 2, 3].map((index) => ({ index, ...result(index === 1 ? 0.9 : index === 2 ? 0.8 : null) })));
    expect(hasMotionBlur(useStore.getState().ai[1])).toBe(true);
    expect(problemOf(useStore.getState().ai[1], undefined, true)).toBe("motionBlur");
    useStore.getState().setMotionBlurFilter(true);
    expect(useStore.getState().view).toEqual([1, 2]);
    useStore.getState().setFlagFilter("rejects");
    expect(useStore.getState().view).toEqual([2]);
    useStore.getState().setRatingFilter(4);
    expect(useStore.getState().view).toEqual([2]);
    useStore.getState().clearFilters();
    expect(useStore.getState().view).toEqual([0, 1, 2, 3]);
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
      ai: [result(null), result(null)],
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
});
