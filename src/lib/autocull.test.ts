import { describe, expect, it } from "vitest";

import { aroundFor, colourable, foreignColours, framesFor, NEAREST, NEAREST_MAX_MS, NEIGHBOUR_MS, ownedAfter } from "./autocull";
import { PLAIN } from "./framing";
import type { AiFace, AiResult, Photo } from "./types";

function photo(index: number, color: Photo["color"] = "none", rating = 0): Photo {
  return {
    path: `/f/${index}.ARW`, display: `/f/${index}.ARW`, name: `${index}.ARW`, subpath: null, version: "v1", size: 1, modified: 0,
    index, rating, flag: "none", color, exif: null, captureMs: index, fullWidth: 10, fullHeight: 10, framing: PLAIN, framable: true, group: null,
  };
}
const result: AiResult = { faces: [], score: 70, focus: 0.8, gaze: 0.9, key: null };

describe("auto-cull frames", () => {
  it("sends only analysed photos, with their burst place and stars", () => {
    const frames = framesFor([photo(0, "none", 5), photo(1)], [result, undefined], [{ pose: 0, score: 72, eyes: [] }, undefined], [{ rank: 1, count: 3 }, undefined], [4, undefined]);
    expect(frames).toHaveLength(1);
    expect(frames[0]).toMatchObject({ path: "/f/0.ARW", comparedScore: 72, rank: 1, count: 3, pose: 4, rating: 5, score: 70 });
  });

  it("sends each photo's capture time and flag", () => {
    const flagged = { ...photo(7), flag: "pick" as const };
    const untimed = { ...photo(8), captureMs: null };
    const frames = framesFor([flagged, untimed], [result, result], [undefined, undefined], [undefined, undefined], [undefined, undefined]);
    expect(frames[0]).toMatchObject({ time: 7, flag: "pick" });
    expect(frames[1]).toMatchObject({ time: null, flag: "none" });
  });

  it("passes a face's open state through into the frame", () => {
    const face: AiFace = { x: 0.4, y: 0.3, w: 0.2, h: 0.2, subject: true, reason: null, eyesShut: false, lookingAway: false, maybeAway: false, sharpness: 80, eye: 0.3, eyesRead: true, state: null, concern: 0, excused: 0, open: true };
    const frames = framesFor([photo(0)], [{ ...result, faces: [face] }], [undefined], [undefined], [undefined]);
    expect(frames[0].faces[0].open).toBe(true);
  });
});

describe("whose colours are whose", () => {
  const owned = { "/f/1.ARW": { version: "v1", color: "red" as const } };

  it("counts colours OpenCull did not set", () => {
    expect(foreignColours([photo(0, "blue"), photo(1, "red"), photo(2)], owned)).toBe(1);
  });

  it("treats a colour changed by hand as the photographer's", () => {
    expect(foreignColours([photo(1, "green")], owned)).toBe(1);
    expect(colourable(photo(1, "green"), owned, false)).toBe(false);
  });

  it("colours uncoloured photos and its own, and others only when allowed", () => {
    expect(colourable(photo(2), owned, false)).toBe(true);
    expect(colourable(photo(1, "red"), owned, false)).toBe(true);
    expect(colourable(photo(0, "blue"), owned, false)).toBe(false);
    expect(colourable(photo(0, "blue"), owned, true)).toBe(true);
  });

  it("remembers what it applied and forgets what it cleared", () => {
    const after = ownedAfter(owned, [{ path: "/f/2.ARW", version: "v1", color: "green" }, { path: "/f/1.ARW", version: "v1", color: "none" }]);
    expect(after).toEqual({ "/f/2.ARW": { version: "v1", color: "green" } });
  });
});

describe("frames for some photos only", () => {
  it("sends only the photos asked for", () => {
    const photos = [photo(0), photo(1), photo(2)];
    const ai = photos.map(() => result);
    const frames = framesFor(photos, ai, [], [], [], new Set([0, 2]));
    expect(frames.map((f) => f.path)).toEqual([photos[0].display, photos[2].display]);
  });

  it("sends them in the photos' order whatever order they are asked for in, and skips unanalysed ones", () => {
    const photos = [photo(0), photo(1), photo(2), photo(3)];
    const ai = [result, result, undefined, result];
    const frames = framesFor(photos, ai, [], [], [], new Set([3, 2, 0, 9]));
    expect(frames.map((f) => f.path)).toEqual([photos[0].display, photos[3].display]);
  });
});

describe("photos shot around the ones judged", () => {
  const at = (index: number, ms: number | null, path = `/f/${index}.ARW`) => ({ ...photo(index), path, display: path, captureMs: ms });

  it("are those within two minutes, not themselves judged", () => {
    const photos = [at(0, 0), at(1, 1_000), at(2, 119_000), at(3, 240_000)];
    expect(aroundFor(photos, new Set([1])).map((a) => a.path)).toEqual(["/f/0.ARW", "/f/2.ARW"]);
    expect(aroundFor(photos, new Set([1]))[0]).toEqual({ path: "/f/0.ARW", version: photos[0].version, time: 0 });
  });

  it("leave out videos and photos with no capture time", () => {
    const photos = [at(0, 0), at(1, 1_000, "/f/clip.MP4"), at(2, null)];
    expect(aroundFor(photos, new Set([0]))).toEqual([]);
  });

  it("are none when nothing judged has a capture time", () => {
    expect(aroundFor([at(0, null), at(1, 5)], new Set([0]))).toEqual([]);
  });

  it("are the same figures the judging uses", () => {
    expect([NEIGHBOUR_MS, NEAREST, NEAREST_MAX_MS]).toEqual([10_000, 4, 120_000]);
  });

  it("include every photo within ten seconds, however many", () => {
    // A judged photo at 50 s in a crowd one second apart: all ten each side within 10 s.
    const photos = Array.from({ length: 101 }, (_, i) => at(i, i * 1000));
    const sent = aroundFor(photos, new Set([50])).map((a) => a.time);
    const within = photos.filter((p) => p.index !== 50 && Math.abs(p.captureMs! - 50_000) <= NEIGHBOUR_MS).map((p) => p.captureMs);
    expect(within).toHaveLength(20);
    for (const t of within) expect(sent).toContain(t);
    // The four nearest on each side are among them, so nothing further is.
    expect(sent).toHaveLength(20);
  });

  it("include the four nearest on each side beyond ten seconds, not more", () => {
    // Lone judged photo at 0; photos every 15 s either side.
    const photos = [at(0, 0), ...Array.from({ length: 6 }, (_, i) => at(i + 1, (i + 1) * 15_000)), ...Array.from({ length: 6 }, (_, i) => at(i + 7, -(i + 1) * 15_000))];
    const sent = aroundFor(photos, new Set([0])).map((a) => a.time).sort((a, b) => a - b);
    expect(sent).toEqual([-60_000, -45_000, -30_000, -15_000, 15_000, 30_000, 45_000, 60_000]);
  });

  it("keep a photo shot at the same moment as the fourth nearest", () => {
    // The judging takes whichever of the two it meets first, so both are sent.
    const photos = [at(0, 0), at(1, 20_000), at(2, 30_000), at(3, 40_000), at(4, 50_000), at(5, 50_000), at(6, 60_000)];
    expect(aroundFor(photos, new Set([0])).map((a) => a.path)).toEqual(["/f/1.ARW", "/f/2.ARW", "/f/3.ARW", "/f/4.ARW", "/f/5.ARW"]);
  });

  it("include none of the nearest further than two minutes away", () => {
    const photos = [at(0, 0), at(1, 120_000), at(2, 120_001), at(3, -120_001)];
    expect(aroundFor(photos, new Set([0])).map((a) => a.path)).toEqual(["/f/1.ARW"]);
  });

  it("are counted from each judged photo, two judged photos far apart each getting their own", () => {
    const photos = [at(0, 0), at(1, 15_000), at(2, 500_000), at(3, 515_000), at(4, 300_000)];
    expect(aroundFor(photos, new Set([0, 2])).map((a) => a.path)).toEqual(["/f/1.ARW", "/f/3.ARW"]);
  });
});
