import { describe, expect, it } from "vitest";
import { initialSelection, selectedReview, sourceWritable } from "./review";
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
  });
});
