import { describe, expect, it } from "vitest";

import { PLAIN } from "./framing";
import { ratedByHand, tallyOf, useRundown, viewedIn } from "./rundown";
import { useStore } from "./store";
import type { AutocullOwned, Photo } from "./types";

function photo(index: number, marks: Partial<Photo> = {}): Photo {
  return { path: `/f/${index}.ARW`, display: `/f/${index}.ARW`, name: `${index}.ARW`, subpath: null, version: "v1", size: 1, modified: 0, index, rating: 0, flag: "none", color: "none", exif: null, captureMs: index, fullWidth: 10, fullHeight: 10, framing: PLAIN, framable: true, group: null, ...marks };
}
const owned = (entries: [number, AutocullOwned["color"]][]): Record<string, AutocullOwned> => Object.fromEntries(entries.map(([i, color]) => [`/f/${i}.ARW`, { version: "v1", color }]));

describe("what counts as rated", () => {
  it("counts a star, a flag or a colour set by hand", () => {
    expect(ratedByHand(photo(0, { rating: 3 }), {})).toBe(true);
    expect(ratedByHand(photo(0, { flag: "pick" }), {})).toBe(true);
    expect(ratedByHand(photo(0, { flag: "reject" }), {})).toBe(true);
    expect(ratedByHand(photo(0, { color: "blue" }), {})).toBe(true);
    expect(ratedByHand(photo(0), {})).toBe(false);
  });

  it("does not count a colour Auto-label set", () => {
    expect(ratedByHand(photo(0, { color: "green" }), owned([[0, "green"]]))).toBe(false);
  });

  it("counts an Auto-label colour changed by hand since", () => {
    expect(ratedByHand(photo(0, { color: "red" }), owned([[0, "green"]]))).toBe(true);
  });

  it("counts a star on a photo Auto-label coloured", () => {
    expect(ratedByHand(photo(0, { color: "green", rating: 1 }), owned([[0, "green"]]))).toBe(true);
  });
});

describe("the folder's tally", () => {
  // Photos 0-2 one pose, 3-4 one burst, 5 and 6 on their own.
  const photos = [photo(0), photo(1, { rating: 2 }), photo(2), photo(3, { color: "yellow" }), photo(4), photo(5, { color: "red" }), photo(6)];
  photos[3].group = 7;
  photos[4].group = 7;
  const poses = [-1, -1, -1, undefined, undefined, null, null];
  const order = photos.map((p) => p.index);

  it("counts photos and groups viewed and rated", () => {
    const t = tallyOf({ photos, poses, order }, new Set(["/f/0.ARW", "/f/6.ARW"]), {});
    expect(t).toEqual({ photos: 7, viewed: 2, rated: 3, groups: 4, groupsViewed: 2, groupsRated: 3 });
  });

  it("leaves Auto-label's colours out of photos and groups rated", () => {
    const t = tallyOf({ photos, poses, order }, new Set(), owned([[3, "yellow"], [5, "red"]]));
    expect(t.rated).toBe(1);
    expect(t.groupsRated).toBe(1);
  });

  it("counts only the photos still in the folder", () => {
    const t = tallyOf({ photos, poses, order: [0, 1, 2] }, new Set(), {});
    expect(t.photos).toBe(3);
    expect(t.groups).toBe(1);
  });
});

describe("viewing", () => {
  const photos = [photo(0), photo(1), photo(2)];
  const open = (folder: string) => useStore.setState({ session: { id: 1, folder } as never, photos, order: [0, 1, 2], view: [0, 1, 2], current: 0, viewMode: "loupe" });

  it("counts each photo shown in the loupe", () => {
    open("/viewed-a");
    useStore.setState({ current: 2 });
    expect([...viewedIn("/viewed-a")].sort()).toEqual(["/f/0.ARW", "/f/2.ARW"]);
  });

  it("does not count photos passed over in the grid", () => {
    open("/viewed-b");
    useStore.setState({ viewMode: "grid" });
    useStore.setState({ current: 1 });
    expect([...viewedIn("/viewed-b")]).toEqual(["/f/0.ARW"]);
  });

  it("puts the rundown away when the photo changes", () => {
    open("/viewed-c");
    useRundown.setState({ open: true });
    useStore.setState({ current: 1 });
    expect(useRundown.getState().open).toBe(false);
  });
});
