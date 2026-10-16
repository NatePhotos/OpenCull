import { describe, expect, it } from "vitest";
import { initialSelection, resolveReview, REVIEW_CHOICES, selectedReview, sourceWritable } from "./review";
const files = [{ path: "/card/A.RAW", flag: "none" }, { path: "/card/B.RAW", flag: "reject" }, { path: "/card/C.JPG", flag: "pick" }] as const;
describe("card review decisions", () => {
  it("initially selects all non-rejects, including unmarked files", () => {
    expect(initialSelection(files)).toEqual(["/card/A.RAW", "/card/C.JPG"]);
  });
  it("imports only explicitly selected members, regardless of visible filters", () => {
    expect(selectedReview(files, ["/card/C.JPG", "/elsewhere"])).toEqual([files[2]]);
  });
  it("never permits source writes in review or in-place sessions", () => {
    expect(sourceWritable(null)).toBe(true);
    expect(sourceWritable({ stage: "review" })).toBe(false);
    expect(sourceWritable({ stage: "in-place" })).toBe(false);
    expect(sourceWritable({ stage: "copied" })).toBe(true);
  });
});

describe("which card photos are copied", () => {
  const photo = (rating: number, flag: "none" | "pick" | "reject", color = "none") => ({ rating, flag, color });
  const photos = [photo(5, "none"), photo(0, "pick"), photo(0, "none"), photo(5, "reject"), photo(3, "none", "red"), photo(4, "none")];
  const order = [0, 1, 2, 3, 4, 5];

  it("starts with what was picked or given five stars, and nothing else", () => {
    const got = resolveReview(REVIEW_CHOICES, photos, order, new Set(), 0);
    expect(got).toEqual([photos[0], photos[1], photos[3]]);
  });

  it("takes a photo that matches any one choice, in the order shown", () => {
    const got = resolveReview({ mode: "ratings", flags: [], ratings: [3, 4], colors: ["red"] }, photos, [5, 4, 3, 2, 1, 0], new Set(), 0);
    expect(got).toEqual([photos[5], photos[4]]);
  });

  it("copies nothing when no choice is made", () => {
    expect(resolveReview({ mode: "ratings", flags: [], ratings: [], colors: [] }, photos, order, new Set(), 0)).toEqual([]);
  });

  it("can take the selection, the photo on screen if none is selected, or every photo", () => {
    expect(resolveReview({ ...REVIEW_CHOICES, mode: "selection" }, photos, order, new Set([2, 4]), 0)).toEqual([photos[2], photos[4]]);
    expect(resolveReview({ ...REVIEW_CHOICES, mode: "selection" }, photos, order, new Set(), 3)).toEqual([photos[3]]);
    expect(resolveReview({ ...REVIEW_CHOICES, mode: "all" }, photos, order, new Set(), 0)).toHaveLength(6);
  });
});
