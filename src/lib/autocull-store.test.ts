import { beforeEach, describe, expect, it, vi } from "vitest";

import { api } from "./api";
import { PLAIN } from "./framing";
import { rankBar, useStore } from "./store";
import type { AiResult, Compared, Judgement, Photo } from "./types";

function photo(index: number, color: Photo["color"] = "none"): Photo {
  return { path: `/f/${index}.ARW`, display: `/f/${index}.ARW`, name: `${index}.ARW`, subpath: null, version: "v1", size: 1, modified: 0, index, rating: 0, flag: "none", color, exif: null, captureMs: index, fullWidth: 10, fullHeight: 10, framing: PLAIN, framable: true, group: null };
}
const result: AiResult = { faces: [], score: 70, focus: 0.8, gaze: 0.9, key: null };

/** Each reset opens a session of its own, so the runs the store works out for one test never reach another. */
let nextSession = 1000;
function reset(photos: Photo[]) {
  useStore.setState({
    session: { id: nextSession++, folder: "/f" } as never, photos, order: photos.map((p) => p.index), view: photos.map((p) => p.index),
    ai: photos.map(() => result), compared: [], ranks: [], poses: [], aiRunning: false, aiAdvanced: true,
    judged: [], judging: 0, judgeRevision: 0, judgeError: null, aiEnabled: true, showWhy: false, autocullAsk: null, autocullBusy: null, autoCullKind: "events", autoCullLastKind: "events", comparing: 0, undoStack: [], redoStack: [], notice: null,
  });
}

function verdict(path: string, color: Judgement["color"], extra: Partial<Judgement> = {}): Judgement {
  return { path, color, place: null, count: null, reject: color === "red", why: "w", ...extra };
}
/** Photos minutes apart, so none is in a run with another. */
function lone(index: number): Photo {
  return { ...photo(index), captureMs: index * 1_000_000 };
}

describe("Auto-label in the store", () => {
  beforeEach(() => { vi.restoreAllMocks(); vi.spyOn(api, "setMarks").mockResolvedValue(undefined as never); vi.spyOn(api, "autocullSetOwned").mockResolvedValue(); });

  it("asks before replacing colours it did not set, and leaves them until told", async () => {
    reset([photo(0, "blue"), photo(1)]);
    vi.spyOn(api, "autocullOwned").mockResolvedValue({});
    useStore.setState({ judged: [verdict("/f/0.ARW", "red"), verdict("/f/1.ARW", "yellow")] });
    await useStore.getState().runAutoCull();
    expect(useStore.getState().autocullAsk).toBe(1);
    expect(useStore.getState().photos.map((p) => p.color)).toEqual(["blue", "none"]);
    await useStore.getState().runAutoCull(false);
    expect(useStore.getState().photos.map((p) => p.color)).toEqual(["blue", "yellow"]);
    await useStore.getState().runAutoCull(true);
    expect(useStore.getState().photos.map((p) => p.color)).toEqual(["red", "yellow"]);
  });

  it("colours with no switch of its own, ignoring an old saved one", async () => {
    reset([photo(0), photo(1)]);
    // A preference saved by an earlier build that had an Auto-label switch.
    useStore.setState({ autoCull: false } as never);
    vi.spyOn(api, "autocullOwned").mockResolvedValue({});
    useStore.setState({ judged: [verdict("/f/0.ARW", "red"), verdict("/f/1.ARW", "green")] });
    await useStore.getState().runAutoCull();
    expect(useStore.getState().photos.map((p) => p.color)).toEqual(["red", "green"]);
  });

  it("does nothing while OpenCull AI is off", async () => {
    reset([photo(0)]);
    useStore.setState({ aiEnabled: false });
    const owned = vi.spyOn(api, "autocullOwned");
    await useStore.getState().runAutoCull();
    expect(owned).not.toHaveBeenCalled();
  });

  it("does nothing while Advanced AI is off", async () => {
    reset([photo(0)]);
    useStore.setState({ aiAdvanced: false });
    const owned = vi.spyOn(api, "autocullOwned");
    await useStore.getState().runAutoCull();
    expect(owned).not.toHaveBeenCalled();
  });

  it("can be undone in one step", async () => {
    reset([photo(0)]);
    vi.spyOn(api, "autocullOwned").mockResolvedValue({});
    useStore.setState({ judged: [verdict("/f/0.ARW", "red")] });
    await useStore.getState().runAutoCull();
    expect(useStore.getState().photos[0].color).toBe("red");
    useStore.getState().undo();
    expect(useStore.getState().photos[0].color).toBe("none");
  });

  it("keeps the colours and says so when it cannot remember which it set", async () => {
    reset([photo(0)]);
    vi.spyOn(api, "autocullOwned").mockResolvedValue({});
    useStore.setState({ judged: [verdict("/f/0.ARW", "red")] });
    vi.spyOn(api, "autocullSetOwned").mockRejectedValue("disk full");
    await useStore.getState().runAutoCull();
    const s = useStore.getState();
    expect(s.photos[0].color).toBe("red");
    expect(s.notice?.title).toBe("Couldn't remember which colours Auto-label set");
    expect(s.notice?.body).toContain("disk full");
    expect(s.autocullBusy).toBeNull();
  });

  it("uses the type being saved, not the one on disk, when run straight after a change", async () => {
    reset([photo(0)]);
    vi.spyOn(api, "autocullSetKind").mockReturnValue(new Promise(() => {}));
    vi.spyOn(api, "autocullKind").mockResolvedValue("events");
    vi.spyOn(api, "autocullOwned").mockResolvedValue({});
    const judge = vi.spyOn(api, "autocullJudge").mockResolvedValue([]);
    void useStore.getState().setAutoCullKind("sports");
    expect(judge).toHaveBeenCalledWith(expect.any(Array), [], false, "sports");
    expect(useStore.getState().autoCullKind).toBe("sports");
  });

  it("waits for burst comparisons to finish before colouring", async () => {
    reset([photo(0), photo(1)]);
    // A session of its own, so the runs worked out for it are its own.
    useStore.setState({ session: { id: 42, folder: "/f" } as never, aiEnabled: true, groupingRevision: 0 });
    let finish!: (value: Compared[][]) => void;
    vi.spyOn(api, "aiCompare").mockReturnValueOnce(new Promise<Compared[][]>((done) => { finish = done; }));
    const judge = vi.spyOn(api, "autocullJudge").mockResolvedValue([verdict("/f/0.ARW", "green"), verdict("/f/1.ARW", "yellow")]);
    const owned = vi.spyOn(api, "autocullOwned").mockResolvedValue({});
    useStore.getState().regroup();
    expect(useStore.getState().comparing).toBe(1);
    await useStore.getState().runAutoCull();
    expect(owned).not.toHaveBeenCalled();
    expect(judge).not.toHaveBeenCalled();
    finish([[{ pose: 0, score: 100, eyes: [] }, { pose: 0, score: 50, eyes: [] }]]);
    await vi.waitFor(() => expect(useStore.getState().comparing).toBe(0));
    expect(useStore.getState().poses).toEqual([-1, -1]);
    // Judging is asked for once the run is compared.
    expect(judge).toHaveBeenCalledOnce();
    expect(judge.mock.calls[0][0].map((f) => f.path)).toEqual(["/f/0.ARW", "/f/1.ARW"]);
    await vi.waitFor(() => expect(useStore.getState().judging).toBe(0));
    await useStore.getState().runAutoCull();
    expect(owned).toHaveBeenCalledOnce();
  });

  it("counts a comparison as finished even when it fails or is no longer wanted", async () => {
    reset([photo(0), photo(1)]);
    useStore.setState({ session: { id: 43, folder: "/f" } as never, aiEnabled: true, groupingRevision: 0 });
    vi.spyOn(api, "aiCompare").mockRejectedValueOnce("broken");
    useStore.getState().regroup();
    expect(useStore.getState().comparing).toBe(1);
    await vi.waitFor(() => expect(useStore.getState().comparing).toBe(0));
  });

  it("shows that it is clearing while it clears", async () => {
    reset([photo(0, "red")]);
    let answer!: (owned: Record<string, { version: string; color: "red" }>) => void;
    vi.spyOn(api, "autocullOwned").mockReturnValueOnce(new Promise((done) => { answer = done; }));
    const clearing = useStore.getState().clearAutoCull();
    expect(useStore.getState().autocullBusy).toBe("clearing");
    answer({ "/f/0.ARW": { version: "v1", color: "red" } });
    await clearing;
    expect(useStore.getState().autocullBusy).toBeNull();
  });

  it("clears only the colours it set", async () => {
    reset([photo(0, "red"), photo(1, "blue"), photo(2, "green")]);
    vi.spyOn(api, "autocullOwned").mockResolvedValue({ "/f/0.ARW": { version: "v1", color: "red" }, "/f/2.ARW": { version: "v1", color: "yellow" } });
    await useStore.getState().clearAutoCull();
    expect(useStore.getState().photos.map((p) => p.color)).toEqual(["none", "blue", "green"]);
    expect(api.autocullSetOwned).toHaveBeenCalledWith("/f", { "/f/2.ARW": { version: "v1", color: "yellow" } });
    expect(useStore.getState().autocullBusy).toBeNull();
  });
});

describe("the new judging in regular culling", () => {
  beforeEach(() => { vi.restoreAllMocks(); vi.spyOn(api, "setMarks").mockResolvedValue(undefined as never); vi.spyOn(api, "autocullSetOwned").mockResolvedValue(); });

  it("judges a photo with no run as soon as it is analysed, and keeps the verdict", async () => {
    reset([lone(0), lone(1)]);
    useStore.setState({ session: { id: 501, folder: "/f" } as never, ai: [] });
    const judge = vi.spyOn(api, "autocullJudge").mockResolvedValue([verdict("/f/0.ARW", "red", { why: "Likely reject: eyes shut" })]);
    useStore.getState().addAnalysis([{ index: 0, ...result }]);
    expect(judge).toHaveBeenCalledWith([expect.objectContaining({ path: "/f/0.ARW" })], [], false, "events");
    await vi.waitFor(() => expect(useStore.getState().judged[0]?.why).toBe("Likely reject: eyes shut"));
    expect(useStore.getState().judging).toBe(0);
  });

  it("judges a run once it is compared, telling it of the photos around", async () => {
    const near = (i: number) => ({ ...photo(i), captureMs: i * 1000 });
    reset([near(0), near(1), { ...photo(2), captureMs: 60_000 }]);
    useStore.setState({ session: { id: 502, folder: "/f" } as never, ai: [result, result, undefined as never], groupingRevision: 0 });
    vi.spyOn(api, "aiCompare").mockResolvedValue([[{ pose: 0, score: 90, eyes: [] }, { pose: 0, score: 60, eyes: [] }]]);
    const judge = vi.spyOn(api, "autocullJudge").mockResolvedValue([]);
    useStore.getState().regroup();
    await vi.waitFor(() => expect(judge).toHaveBeenCalled());
    const [frames, around] = judge.mock.calls[0];
    expect(frames.map((f) => f.path)).toEqual(["/f/0.ARW", "/f/1.ARW"]);
    expect(around).toEqual([{ path: "/f/2.ARW", version: "v1", time: 60_000 }]);
  });

  it("asks nothing with Basic AI", () => {
    reset([lone(0)]);
    useStore.setState({ session: { id: 503, folder: "/f" } as never, ai: [], aiAdvanced: false });
    const judge = vi.spyOn(api, "autocullJudge").mockResolvedValue([]);
    useStore.getState().addAnalysis([{ index: 0, ...result }]);
    expect(judge).not.toHaveBeenCalled();
  });

  it("drops a verdict asked for under the previous shoot type, and asks again under the new one", async () => {
    reset([lone(0)]);
    useStore.setState({ session: { id: 504, folder: "/f" } as never, ai: [] });
    vi.spyOn(api, "autocullSetKind").mockResolvedValue();
    let first!: (v: Judgement[]) => void;
    const judge = vi.spyOn(api, "autocullJudge")
      .mockReturnValueOnce(new Promise((done) => { first = done; }))
      .mockResolvedValueOnce([verdict("/f/0.ARW", "none", { why: "sports" })]);
    useStore.getState().addAnalysis([{ index: 0, ...result }]);
    await useStore.getState().setAutoCullKind("sports");
    expect(judge).toHaveBeenLastCalledWith(expect.any(Array), [], false, "sports");
    first([verdict("/f/0.ARW", "red", { why: "events" })]);
    await vi.waitFor(() => expect(useStore.getState().judging).toBe(0));
    expect(useStore.getState().judged[0]?.why).toBe("sports");
  });

  it("forgets the verdicts when the photos are grouped again", () => {
    reset([lone(0)]);
    useStore.setState({ session: { id: 505, folder: "/f" } as never, judged: [verdict("/f/0.ARW", "red")] });
    vi.spyOn(api, "autocullJudge").mockResolvedValue([]);
    useStore.getState().regroup();
    expect(useStore.getState().judged).toEqual([]);
  });

  it("Auto-label writes the verdicts' colours without judging again", async () => {
    reset([photo(0), photo(1)]);
    vi.spyOn(api, "autocullOwned").mockResolvedValue({});
    const judge = vi.spyOn(api, "autocullJudge");
    useStore.setState({ judged: [verdict("/f/0.ARW", "red"), verdict("/f/1.ARW", "green")] });
    await useStore.getState().runAutoCull();
    expect(judge).not.toHaveBeenCalled();
    expect(useStore.getState().photos.map((p) => p.color)).toEqual(["red", "green"]);
  });

  it("Auto-label waits while photos are being judged", async () => {
    reset([photo(0)]);
    const owned = vi.spyOn(api, "autocullOwned").mockResolvedValue({});
    useStore.setState({ judging: 1, judged: [verdict("/f/0.ARW", "red")] });
    await useStore.getState().runAutoCull();
    expect(owned).not.toHaveBeenCalled();
    expect(useStore.getState().photos[0].color).toBe("none");
  });

  it("makes a failed judging known, and Auto-label then colours nothing and asks again", async () => {
    reset([lone(0)]);
    useStore.setState({ session: { id: 507, folder: "/f" } as never, ai: [] });
    const judge = vi.spyOn(api, "autocullJudge").mockRejectedValueOnce("model missing").mockResolvedValue([]);
    const owned = vi.spyOn(api, "autocullOwned").mockResolvedValue({});
    useStore.getState().addAnalysis([{ index: 0, ...result }]);
    await vi.waitFor(() => expect(useStore.getState().judgeError).toBe("model missing"));
    expect(useStore.getState().judging).toBe(0);
    await useStore.getState().runAutoCull();
    const s = useStore.getState();
    expect(owned).not.toHaveBeenCalled();
    expect(s.photos[0].color).toBe("none");
    expect(s.notice?.title).toBe("Auto-label did not finish");
    expect(s.notice?.body).toContain("model missing");
    expect(s.autocullBusy).toBeNull();
    expect(s.judgeError).toBeNull();
    expect(judge).toHaveBeenCalledTimes(2);
  });

  it("colours nothing while an analysed photo has no verdict yet, and asks for it", async () => {
    reset([lone(0), lone(1)]);
    useStore.setState({ session: { id: 508, folder: "/f" } as never, judged: [verdict("/f/0.ARW", "red")] });
    const owned = vi.spyOn(api, "autocullOwned").mockResolvedValue({});
    const judge = vi.spyOn(api, "autocullJudge").mockResolvedValue([]);
    await useStore.getState().runAutoCull();
    const s = useStore.getState();
    expect(owned).not.toHaveBeenCalled();
    expect(s.photos.map((p) => p.color)).toEqual(["none", "none"]);
    expect(s.notice?.body).toBe("Some photos haven't been judged yet. OpenCull is judging them now; press Auto-label once it's done.");
    expect(s.autocullBusy).toBeNull();
    expect(judge).toHaveBeenCalled();
  });

  it("colours what is judged and skips photos whose run was never compared, without asking again", async () => {
    // A run whose comparison failed, a run with a photo that could not be analysed, and a judged photo on its own.
    const at = (i: number, ms: number) => ({ ...photo(i), captureMs: ms });
    reset([at(0, 0), at(1, 1000), at(2, 600_000), at(3, 601_000), at(4, 1_200_000)]);
    useStore.setState({
      session: { id: 510, folder: "/f" } as never,
      ai: [result, result, result, undefined as never, result],
      compared: [],
      judged: [undefined, undefined, undefined, undefined, verdict("/f/4.ARW", "red")],
    });
    vi.spyOn(api, "autocullOwned").mockResolvedValue({});
    const judge = vi.spyOn(api, "autocullJudge").mockResolvedValue([]);
    await useStore.getState().runAutoCull();
    const s = useStore.getState();
    expect(s.photos.map((p) => p.color)).toEqual(["none", "none", "none", "none", "red"]);
    expect(s.notice).toBeNull();
    expect(judge).not.toHaveBeenCalled();
    expect(s.autocullBusy).toBeNull();
  });

  it("colours nothing and says nothing when the verdicts change while it reads which colours are its own", async () => {
    for (const change of [
      () => useStore.setState((s) => ({ judgeRevision: s.judgeRevision + 1 })),
      () => useStore.setState({ judging: 1 }),
      () => useStore.setState({ session: { id: nextSession++, folder: "/f" } as never }),
    ]) {
      reset([photo(0, "blue"), photo(1)]);
      useStore.setState({ judged: [verdict("/f/0.ARW", "red"), verdict("/f/1.ARW", "green")] });
      vi.spyOn(api, "autocullOwned").mockImplementation(async () => { change(); return {}; });
      const setOwned = vi.spyOn(api, "autocullSetOwned").mockResolvedValue();
      await useStore.getState().runAutoCull(true);
      const s = useStore.getState();
      expect(s.photos.map((p) => p.color)).toEqual(["blue", "none"]);
      expect(s.notice).toBeNull();
      expect(s.autocullAsk).toBeNull();
      expect(s.autocullBusy).toBeNull();
      expect(setOwned).not.toHaveBeenCalled();
      // Asked without an answer to replace foreign colours: still no question.
      reset([photo(0, "blue"), photo(1)]);
      useStore.setState({ judged: [verdict("/f/0.ARW", "red"), verdict("/f/1.ARW", "green")] });
      vi.spyOn(api, "autocullOwned").mockImplementation(async () => { change(); return {}; });
      await useStore.getState().runAutoCull();
      expect(useStore.getState().autocullAsk).toBeNull();
    }
  });

  it("does nothing with OpenCull AI off", async () => {
    reset([photo(0)]);
    useStore.setState({ aiEnabled: false, judged: [verdict("/f/0.ARW", "red")] });
    const owned = vi.spyOn(api, "autocullOwned").mockResolvedValue({});
    await useStore.getState().runAutoCull();
    expect(owned).not.toHaveBeenCalled();
    expect(useStore.getState().photos[0].color).toBe("none");
    expect(useStore.getState().notice).toBeNull();
  });

  it("does not report a failure that comes back after the verdicts stopped counting", async () => {
    reset([lone(0)]);
    useStore.setState({ session: { id: 509, folder: "/f" } as never, ai: [] });
    let fail!: (e: unknown) => void;
    vi.spyOn(api, "autocullJudge").mockReturnValueOnce(new Promise((_, no) => { fail = no; })).mockResolvedValue([]);
    useStore.getState().addAnalysis([{ index: 0, ...result }]);
    useStore.getState().regroup();
    fail("too late");
    await vi.waitFor(() => expect(useStore.getState().judging).toBe(0));
    expect(useStore.getState().judgeError).toBeNull();
  });

});

describe("a thumbnail's strip", () => {
  it("is the burst comparison's with no verdict, red for shut eyes", () => {
    expect(rankBar(undefined, { rank: 2, count: 3 }, null)).toEqual({ rank: { rank: 2, count: 3 }, reject: false, title: "Rank 2 of 3" });
    expect(rankBar(undefined, undefined, "eyesClosed")).toEqual({ rank: undefined, reject: true, title: "Eyes closed" });
    expect(rankBar(undefined, undefined, null)).toBeNull();
  });

  it("follows the verdict once there is one", () => {
    const v = verdict("/a", "green", { place: 1, count: 4, reject: false });
    expect(rankBar(v, { rank: 3, count: 4 }, "eyesClosed")).toEqual({ rank: { rank: 1, count: 4 }, reject: false, title: "Best of these 4" });
  });

  it("is red with the reason for a reject, numbered only in a burst", () => {
    expect(rankBar(verdict("/a", "red", { place: 4, count: 4, why: "Likely reject: blown out" }), undefined, null)).toEqual({ rank: { rank: 4, count: 4 }, reject: true, title: "Likely reject: blown out" });
    expect(rankBar(verdict("/a", "red", { why: "Likely reject: looks like a test shot" }), undefined, null)).toEqual({ rank: undefined, reject: true, title: "Likely reject: looks like a test shot" });
    expect(rankBar(verdict("/a", "none"), undefined, null)).toBeNull();
  });
});

describe("a folder's shoot type", () => {
  const open = (path: string) => {
    vi.spyOn(api, "openFolder").mockResolvedValue({ session: 77, images: [], times: [], groups: null, subfolderCount: 0, leftOut: 0, timed: true } as never);
    vi.spyOn(api, "closeFolder").mockResolvedValue(undefined as never);
    return useStore.getState().openFolder(path);
  };
  beforeEach(() => { vi.restoreAllMocks(); useStore.setState({ session: null, photos: [], aiAdvanced: true, judgeRevision: 0, autoCullKind: "events", autoCullLastKind: "portraits", recents: [] }); });

  it("is loaded when a folder opens", async () => {
    const kind = vi.spyOn(api, "autocullKind").mockResolvedValue("sports");
    await open("/shoot");
    await vi.waitFor(() => expect(useStore.getState().autoCullKind).toBe("sports"));
    expect(kind).toHaveBeenCalledWith("/shoot");
  });

  it("falls back to the last type chosen when the folder has none", async () => {
    vi.spyOn(api, "autocullKind").mockResolvedValue(null);
    await open("/shoot");
    await vi.waitFor(() => expect(useStore.getState().autoCullKind).toBe("portraits"));
  });

  it("falls back to the last type chosen when it cannot be read", async () => {
    vi.spyOn(api, "autocullKind").mockRejectedValue("nope");
    await open("/shoot");
    await vi.waitFor(() => expect(useStore.getState().autoCullKind).toBe("portraits"));
  });

  it("is the folder's own, never the previous folder's", async () => {
    useStore.setState({ autoCullKind: "sports" });
    vi.spyOn(api, "autocullKind").mockResolvedValue(null);
    await open("/shoot");
    expect(useStore.getState().autoCullKind).toBe("portraits");
  });

  it("does not overwrite a type chosen since the folder opened", async () => {
    let answer!: (k: "sports" | null) => void;
    vi.spyOn(api, "autocullKind").mockReturnValue(new Promise((done) => { answer = done; }));
    vi.spyOn(api, "autocullSetKind").mockResolvedValue();
    await open("/shoot");
    await useStore.getState().setAutoCullKind("events");
    answer("sports");
    await new Promise((r) => setTimeout(r, 0));
    expect(useStore.getState().autoCullKind).toBe("events");
  });

  it("is read from the folder when Advanced AI is switched on after it opened", async () => {
    vi.spyOn(api, "autocullKind").mockResolvedValue("sports");
    useStore.setState({ aiAdvanced: false });
    await open("/shoot");
    expect(useStore.getState().autoCullKind).toBe("portraits");
    useStore.getState().set("aiAdvanced", true);
    await vi.waitFor(() => expect(useStore.getState().autoCullKind).toBe("sports"));
  });

  it("is not looked up with Basic AI", async () => {
    useStore.setState({ aiAdvanced: false });
    const kind = vi.spyOn(api, "autocullKind").mockResolvedValue("sports");
    await open("/shoot");
    expect(kind).not.toHaveBeenCalled();
  });

  it("judges the folder again when its saved type turns out to differ", async () => {
    vi.spyOn(api, "autocullKind").mockResolvedValue("sports");
    vi.spyOn(api, "autocullJudge").mockResolvedValue([]);
    await open("/shoot");
    await vi.waitFor(() => expect(useStore.getState().autoCullKind).toBe("sports"));
    expect(useStore.getState().judgeRevision).toBe(1);
  });

  it("is saved with the folder and remembered as the last choice", async () => {
    useStore.setState({ session: { id: 1, folder: "/f" } as never });
    const set = vi.spyOn(api, "autocullSetKind").mockResolvedValue();
    await useStore.getState().setAutoCullKind("sports");
    expect(set).toHaveBeenCalledWith("/f", "sports");
    expect(useStore.getState().autoCullKind).toBe("sports");
    expect(useStore.getState().autoCullLastKind).toBe("sports");
  });
});

describe("the AI bar", () => {
  it("is closed by default, and remembered once opened", async () => {
    localStorage.removeItem("opencull.showAiBar");
    vi.resetModules();
    const first = (await import("./store")).useStore;
    expect(first.getState().showAiBar).toBe(false);
    first.getState().toggle("showAiBar");
    expect(localStorage.getItem("opencull.showAiBar")).toBe("true");
    vi.resetModules();
    const next = (await import("./store")).useStore;
    expect(next.getState().showAiBar).toBe(true);
    localStorage.removeItem("opencull.showAiBar");
  });
});
