import { expect, it } from "vitest";
import { isVideo } from "./media";
it("recognizes camera video filenames without mistaking stills", () => {
  for (const p of ["A.MOV", "a.mp4", "/card/B.MTS", "c.m2ts", "d.webm", "e.mkv"]) expect(isVideo(p)).toBe(true);
  for (const p of ["a.ARW", "b.JPG", "photo.mp4.jpg", "/video.mp4/photo.raw"]) expect(isVideo(p)).toBe(false);
});
