import { describe, expect, it } from "vitest";
import { filtered, groupOf, groupsOf, problemOf, rankRun, runsOf, sorted, type Filters } from "./cull-logic";
import type { AiFace, AiResult, Compared, Photo, Rank } from "./types";

/** A photo with nothing marked, taken at `captureMs`. */
function photo(index: number, over: Partial<Photo> = {}): Photo {
  return {
    path: `/shoot/P${index}.ARW`,
    display: `/shoot/P${index}.ARW`,
    name: `P${index}.ARW`,
    subpath: null,
    version: "1",
    size: 1,
    modified: 0,
    index,
    rating: 0,
    flag: "none",
    color: "none",
    exif: null,
    captureMs: index * 1000,
    fullWidth: 0,
    fullHeight: 0,
    group: null,
    ...over,
  };
}

const none: Filters = { flagFilter: "all", ratingFilter: null, colorFilters: new Set() };

function face(over: Partial<AiFace> = {}): AiFace {
  return { x: 0, y: 0, w: 0.1, h: 0.1, subject: true, reason: null, eyesShut: false, lookingAway: false, sharpness: 80, eye: 0.3, eyesRead: true, ...over };
}

function result(faces: AiFace[]): AiResult {
  return { failed: false, faces, score: 50, focus: null, gaze: null, key: null };
}

describe("sorted", () => {
  it("breaks ties in capture time on scan order, whichever way it runs", () => {
    const photos = [photo(0, { captureMs: 5 }), photo(1, { captureMs: 5 }), photo(2, { captureMs: 1 })];
    expect(sorted(photos, "captureTime", true)).toEqual([2, 0, 1]);
    expect(sorted(photos, "captureTime", false)).toEqual([0, 1, 2]);
  });

  it("puts a photo with no time before everything", () => {
    const photos = [photo(0, { captureMs: 5 }), photo(1, { captureMs: null })];
    expect(sorted(photos, "captureTime", true)).toEqual([1, 0]);
  });

  it("sorts by rating and keeps shot order within a rating", () => {
    const photos = [photo(0, { rating: 3 }), photo(1, { rating: 1 }), photo(2, { rating: 3 })];
    expect(sorted(photos, "rating", false)).toEqual([0, 2, 1]);
  });

  it("is scan order for filenames", () => {
    const photos = [photo(0), photo(1), photo(2)];
    expect(sorted(photos, "filename", false)).toEqual([2, 1, 0]);
  });
});

describe("filtered", () => {
  const photos = [photo(0, { flag: "pick", rating: 3, color: "red" }), photo(1, { flag: "reject" }), photo(2, { rating: 3 })];

  it("lets everything through with no filter, as the same array", () => {
    const order = [0, 1, 2];
    expect(filtered(order, photos, none)).toBe(order);
  });

  it("needs every active filter to agree", () => {
    expect(filtered([0, 1, 2], photos, { ...none, ratingFilter: 3 })).toEqual([0, 2]);
    expect(filtered([0, 1, 2], photos, { ...none, ratingFilter: 3, flagFilter: "picks" })).toEqual([0]);
    expect(filtered([0, 1, 2], photos, { ...none, colorFilters: new Set(["red"] as const) })).toEqual([0]);
    expect(filtered([0, 1, 2], photos, { ...none, flagFilter: "unflagged" })).toEqual([2]);
  });
});

describe("runsOf", () => {
  it("splits where the gap between frames is more than twenty seconds", () => {
    const photos = [photo(0, { captureMs: 0 }), photo(1, { captureMs: 19_000 }), photo(2, { captureMs: 60_000 }), photo(3, { captureMs: null })];
    expect(runsOf(photos)).toEqual([[0, 1], [2]]);
  });
});

describe("rankRun", () => {
  it("ranks each pose best first and leaves a lone frame unranked", () => {
    const ranks: (Rank | undefined)[] = [{ rank: 9, count: 9 }];
    const compared = [
      { pose: 0, score: 40 },
      { pose: 0, score: 90 },
      { pose: 1, score: 70 },
    ] as Compared[];
    rankRun(ranks, [0, 1, 2], compared);
    expect(ranks[1]).toEqual({ rank: 1, count: 2 });
    expect(ranks[0]).toEqual({ rank: 2, count: 2 });
    expect(ranks[2]).toBeUndefined();
  });

  it("puts a frame with no mark last", () => {
    const ranks: (Rank | undefined)[] = [];
    rankRun(ranks, [0, 1], [{ pose: 0, score: null }, { pose: 0, score: 10 }] as Compared[]);
    expect(ranks[1]?.rank).toBe(1);
    expect(ranks[0]?.rank).toBe(2);
  });
});

describe("problemOf", () => {
  it("names the worst thing wrong with the subjects", () => {
    expect(problemOf(result([face({ eyesShut: true }), face({ lookingAway: true })]))).toBe("eyesClosed");
    expect(problemOf(result([face({ lookingAway: true })]))).toBe("lookingAway");
    expect(problemOf(result([face({ sharpness: 20 })]))).toBe("softFocus");
    expect(problemOf(result([face()]))).toBeNull();
  });

  it("ignores people in the background", () => {
    expect(problemOf(result([face({ subject: false, eyesShut: true })]))).toBeNull();
  });

  it("sees a blink against the same person's best in the burst", () => {
    expect(problemOf(result([face()]), { pose: 0, score: 50, eyes: [0.4] } as Compared)).toBe("blinking");
  });

  it("has nothing to say about a photo that could not be analysed", () => {
    expect(problemOf({ ...result([]), failed: true })).toBeNull();
  });
});

describe("groups", () => {
  it("goes by pose once compared, and by burst until then", () => {
    const photos = [photo(0, { group: 7 }), photo(1, { group: 7 })];
    expect(groupOf({ photos, poses: [] }, 0)).toBe(7);
    expect(groupOf({ photos, poses: [-1, null] }, 0)).toBe(-1);
    expect(groupOf({ photos, poses: [-1, null] }, 1)).toBeNull();
  });

  it("hands back the same array while no group has changed", () => {
    const photos = [photo(0, { group: 1 }), photo(1, { group: 1 })];
    const poses: (number | null | undefined)[] = [];
    const first = groupsOf({ photos, poses });
    // A mark makes a new photos array; the groups are the same.
    const marked = photos.map((p, i) => (i === 0 ? { ...p, rating: 5 } : p));
    expect(groupsOf({ photos: marked, poses })).toBe(first);
    const regrouped = photos.map((p, i) => (i === 1 ? { ...p, group: 2 } : p));
    expect(groupsOf({ photos: regrouped, poses })).toEqual([1, 2]);
  });
});
