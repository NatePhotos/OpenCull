import { beforeEach, expect, it, vi } from "vitest";
const api = vi.hoisted(() => ({ setMarks: vi.fn() }));
vi.mock("./api", () => ({ api, inTauri: false }));
import { hasMotionBlur, motionAdjustedScore } from "./motion-blur";
import { problemOf, rankRun, useStore } from "./store";
import { newReview } from "./review";
import { PLAIN } from "./framing";
import { exportSettings, parseSettingsImport } from "./settings-transfer";
import type { AiResult, Photo, Rank } from "./types";

const result = (motionBlur: number | null): AiResult => ({ faces: [], score: 90, focus: null, gaze: null, key: null, motionBlur });
const photo = (index: number, rating = 0): Photo => ({ path: `/card/${index}.RAW`, display: `/card/${index}.JPG`, name: `${index}.RAW`, subpath: null, version: "1", size: 10, modified: 0, index, rating, flag: "none", color: "none", exif: null, captureMs: null, fullWidth: 200, fullHeight: 200, framing: PLAIN, framable: true, group: null });
beforeEach(() => {
  vi.clearAllMocks();
  const photos = [photo(0), photo(1, 5), photo(2)];
  useStore.setState({ photos, review: newReview(["/card"], photos), session: null, ai: [], aiDone: 0, order: [0, 1, 2], view: [0, 1, 2], current: 0, selected: new Set(), flagFilter: "all", ratingFilter: null, colorFilters: new Set(), motionBlurFilter: false, motionRatingSkipped: new Set(), aiEnabled: true, aiMotionBlur: true, autoRateMotionBlur: true, metaProgress: 1, selfTest: false, embedInJpeg: false, undoStack: [], redoStack: [], speedCullAlways: true });
});
it("requires valid motion evidence rather than treating low focus as motion", () => {
  for (const value of [null, NaN, Infinity, -1, 0.79, 2]) expect(hasMotionBlur(result(value))).toBe(false);
  expect(hasMotionBlur(result(0.9))).toBe(true);
  expect(problemOf(result(0.9), undefined, true)).toBe("motionBlur");
  expect(problemOf(result(0.9))).toBeNull();
});
it("lowers group rank only when detection is enabled", () => {
  const ranks: (Rank | undefined)[] = [];
  rankRun(ranks, [0, 1], [{ pose: 0, eyes: [], score: motionAdjustedScore(90, result(0.9), true) }, { pose: 0, eyes: [], score: 75 }]);
  expect(ranks[0]?.rank).toBe(2); expect(ranks[1]?.rank).toBe(1);
  expect(motionAdjustedScore(90, result(0.9), false)).toBe(90);
});
it("rates and rejects only unrated detections, without advancing or writing to a card", () => {
  useStore.getState().addAnalysis([{ index: 0, ...result(0.9) }, { index: 1, ...result(0.9) }, { index: 2, ...result(0.1) }]);
  expect(useStore.getState().photos.map((p) => p.rating)).toEqual([1, 5, 0]);
  expect(useStore.getState().photos.map(p => p.flag)).toEqual(["reject", "none", "none"]);
  expect(useStore.getState().current).toBe(0);
  expect(useStore.getState().review?.decisions["/card/0.RAW"].rating).toBe(1);
  expect(useStore.getState().review?.decisions["/card/0.RAW"].flag).toBe("reject");
  expect(api.setMarks).not.toHaveBeenCalled();
  useStore.getState().undo();
  useStore.getState().addAnalysis([{ index: 0, ...result(0.9) }]);
  expect(useStore.getState().photos[0].rating).toBe(0);
  expect(useStore.getState().photos[0].flag).toBe("none");
});
it("honours manual ratings and persists a normal-folder automatic rating through the usual writer", () => {
  useStore.getState().setRating(0, [0]);
  useStore.getState().addAnalysis([{ index: 0, ...result(0.9) }]);
  expect(useStore.getState().photos[0].rating).toBe(0);
  useStore.setState({ review: null, motionRatingSkipped: new Set() });
  useStore.getState().rateMotionBlur();
  expect(api.setMarks).toHaveBeenCalledWith([expect.objectContaining({ path: "/card/0.RAW", rating: 1, flag: "reject", color: "none" })]);
});
it("combines motion, star and colour filters, and clears the motion filter", () => {
  useStore.getState().addAnalysis([{ index: 0, ...result(0.9) }, { index: 1, ...result(0.9) }, { index: 2, ...result(null) }]);
  useStore.getState().setMotionBlurFilter(true);
  expect(useStore.getState().view).toEqual([0, 1]);
  useStore.getState().setRatingFilter(5);
  expect(useStore.getState().view).toEqual([1]);
  useStore.getState().clearFilters();
  expect(useStore.getState().view).toEqual([0, 1, 2]);
});
it("does not rate before metadata loads or when automation is disabled; transfers preferences only", () => {
  useStore.setState({ ai: [result(0.9)], metaProgress: 0 });
  useStore.getState().rateMotionBlur(); expect(useStore.getState().photos[0].rating).toBe(0);
  useStore.setState({ metaProgress: 1, autoRateMotionBlur: false });
  useStore.getState().rateMotionBlur(); expect(useStore.getState().photos[0].rating).toBe(0);
  expect(parseSettingsImport(exportSettings({ aiMotionBlur: true, autoRateMotionBlur: false, motionRatingSkipped: new Set([0]) }))).toEqual({ aiMotionBlur: true, autoRateMotionBlur: false });
});

it("preserves an explicit pick even when the picked photo is unrated", () => {
  const photos = useStore.getState().photos.slice();
  photos[0] = { ...photos[0], flag: "pick", color: "blue" };
  useStore.setState({ photos });
  useStore.getState().addAnalysis([{ index: 0, ...result(.9) }]);
  expect(useStore.getState().photos[0]).toMatchObject({ rating: 0, flag: "pick", color: "blue" });
  expect(api.setMarks).not.toHaveBeenCalled();
});
