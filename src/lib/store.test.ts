import { beforeEach, describe, expect, it } from "vitest";
import { useStore } from "./store";
import type { Photo } from "./types";

// Runs against the in-browser stand-in for the app shell (mock.ts), which
// api.ts picks when not inside Tauri.

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

/** A folder of `photos` open in the store, on the first one. */
function open(photos: Photo[]) {
  const order = photos.map((p) => p.index);
  useStore.setState({
    session: { id: 1, folder: "/shoot", name: "shoot", includeSubfolders: false, subfolderCount: 0, fileKinds: "both", leftOut: 0, timed: true },
    photos,
    order,
    view: order,
    current: order[0] ?? -1,
    anchor: order[0] ?? 0,
    selected: new Set(),
    undoStack: [],
    redoStack: [],
    flagFilter: "all",
    ratingFilter: null,
    colorFilters: new Set(),
    poses: [],
    speedCull: false,
    speedCullAlways: false,
  });
}

const s = () => useStore.getState();

describe("marks, undo and redo", () => {
  beforeEach(() => open([photo(0), photo(1), photo(2)]));

  it("undoes and redoes a rating, and goes back to the photo it changed", () => {
    s().setRating(4);
    s().go(2);
    expect(s().photos[0].rating).toBe(4);

    s().undo();
    expect(s().photos[0].rating).toBe(0);
    expect(s().current).toBe(0);

    s().go(2);
    s().redo();
    expect(s().photos[0].rating).toBe(4);
    expect(s().current).toBe(0);
  });

  it("records nothing for a mark that changes nothing", () => {
    s().setRating(0);
    expect(s().undoStack).toHaveLength(0);
  });

  it("forgets what could be redone once something new is marked", () => {
    s().setRating(2);
    s().undo();
    s().setFlag("pick");
    expect(s().redoStack).toHaveLength(0);
  });

  it("keeps at most two hundred steps", () => {
    for (let i = 0; i < 205; i++) s().setRating(i % 2 === 0 ? 3 : 4);
    expect(s().undoStack).toHaveLength(200);
  });

  it("moves on after a mark in speed cull, and not otherwise", () => {
    s().setRating(1);
    expect(s().current).toBe(0);
    useStore.setState({ speedCullAlways: true });
    s().setRating(2);
    expect(s().current).toBe(1);
  });
});

describe("filters", () => {
  it("moves off a photo the filter has just hidden, to the next one that shows", () => {
    open([photo(0, { flag: "pick" }), photo(1), photo(2, { flag: "pick" })]);
    s().go(1);
    s().setFlagFilter("picks");
    expect(s().view).toEqual([0, 2]);
    expect(s().current).toBe(2);
  });

  it("drops a photo from the view as soon as a mark stops it passing", () => {
    open([photo(0), photo(1), photo(2)]);
    s().setRatingFilter(0);
    s().setRating(3);
    expect(s().view).toEqual([1, 2]);
  });
});

describe("moving by group", () => {
  beforeEach(() =>
    open([photo(0, { group: 1 }), photo(1, { group: 1 }), photo(2), photo(3, { group: 2 }), photo(4, { group: 2 }), photo(5, { group: 2 })]),
  );

  it("goes to the first frame of the next group", () => {
    s().nextGroup();
    expect(s().current).toBe(2);
    s().nextGroup();
    expect(s().current).toBe(3);
  });

  it("goes back to the first frame of the group before", () => {
    s().go(5);
    s().previousGroup();
    expect(s().current).toBe(2);
    s().previousGroup();
    expect(s().current).toBe(0);
  });
});

describe("selection", () => {
  it("selects a range in the order shown, not the order in the folder", () => {
    open([photo(0, { flag: "pick" }), photo(1), photo(2, { flag: "pick" }), photo(3, { flag: "pick" })]);
    s().setFlagFilter("picks");
    s().select(0, { meta: false, shift: false });
    s().select(3, { meta: false, shift: true });
    expect([...s().selected].sort()).toEqual([0, 2, 3]);
  });
});

describe("re-sort", () => {
  it("leaves the order alone when a photo is marked, and brings it up to date when asked", () => {
    open([photo(0), photo(1), photo(2)]);
    s().setSort("rating", false);
    s().go(2);
    s().setRating(5);
    expect(s().order).toEqual([0, 1, 2]);
    s().resort();
    expect(s().order).toEqual([2, 0, 1]);
  });
});

describe("where each folder was left", () => {
  it("is forgotten when the folder leaves the recents, and moves with a folder that moved", () => {
    useStore.setState({ recents: ["/a", "/b"] });
    localStorage.setItem("opencull.position./b", JSON.stringify("/b/P1.ARW"));
    localStorage.setItem("opencull.position./a", JSON.stringify("/a/P2.ARW"));

    s().removeRecents(["/b"]);
    expect(localStorage.getItem("opencull.position./b")).toBeNull();

    s().relocateRecent("/a", "/moved/a");
    expect(JSON.parse(localStorage.getItem("opencull.position./moved/a")!)).toBe("/moved/a/P2.ARW");
    expect(localStorage.getItem("opencull.position./a")).toBeNull();
  });
});
