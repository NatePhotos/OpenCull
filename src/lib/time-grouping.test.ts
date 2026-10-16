import { expect, it, vi } from "vitest";
vi.mock("./api", () => ({ api: {}, inTauri: false }));
import { runsOf } from "./store";
import type { Photo } from "./types";
it("keeps a thirteen-second change of moment out of the same comparison run", () => {
  const photos = [0, 1000, 14000, 15000].map((captureMs, index) => ({ captureMs, index }) as Photo);
  expect(runsOf(photos)).toEqual([[0, 1], [2, 3]]);
  expect(runsOf([{ captureMs: null, index: 0 } as Photo])).toEqual([]);
});
