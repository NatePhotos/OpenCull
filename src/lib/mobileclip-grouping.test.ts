import { beforeEach, expect, it, vi } from "vitest";
const api = vi.hoisted(() => ({ aiCompare: vi.fn() }));
vi.mock("./api", () => ({ api, inTauri: false }));
import { useStore } from "./store";
import type { AiResult, Compared, Photo } from "./types";
let sessionId = 100;
const ai: AiResult = { faces: [], score: 80, focus: null, gaze: null, key: null };
const marks = (same: boolean): Compared[][] => [[{ pose: 0, score: 80, eyes: [] }, { pose: same ? 0 : 1, score: 70, eyes: [] }]];
beforeEach(() => {
  api.aiCompare.mockReset().mockResolvedValue([]);
  useStore.setState({ session: { id: ++sessionId } as never, photos: [0,1].map(index => ({ index, captureMs:index*200, group:1, fullWidth:6000, fullHeight:4000, framing:{turn:0,crop:null} }) as Photo), ai:[ai,ai], metaProgress:1, aiEnabled:true, aiMobileClipGrouping:false, mobileClipGrouping:"normal", smartGroups:true, groupingRevision:0, aiGroupingBusy:0, aiGroupingError:null, aiMotionBlur:false, compared:[], ranks:[], poses:[] });
});
it("sends only session indices, regroups cached analysis and ignores a late visual reply after disabling", async () => {
  let resolve!: (value: Compared[][]) => void;
  api.aiCompare.mockImplementation((runs: unknown[], _poses: boolean, options: { mobileClip: boolean }) => !runs.length ? Promise.resolve([]) : options.mobileClip ? new Promise(r => {resolve=r;}) : Promise.resolve(marks(true)));
  useStore.setState({aiMobileClipGrouping:true}); useStore.getState().regroup();
  expect(useStore.getState().aiGroupingBusy).toBe(1);
  const call=api.aiCompare.mock.calls.find(([runs]) => runs.length);
  expect(call?.[0][0][0]).toMatchObject({index:0});
  expect(call?.[0][0][0]).not.toHaveProperty("display");
  expect(call?.[2]).toMatchObject({session:sessionId,mobileClip:true,revision:1});
  useStore.setState({aiMobileClipGrouping:false});useStore.getState().regroup();
  await vi.waitFor(() => expect(useStore.getState().poses).toEqual([-1,-1]));
  resolve(marks(false));await Promise.resolve();await Promise.resolve();
  expect(useStore.getState().poses).toEqual([-1,-1]);
  expect(useStore.getState().aiGroupingBusy).toBe(0);
});
it("shows model failures and finishes its busy state instead of claiming visual grouping succeeded", async () => {
  api.aiCompare.mockImplementation((runs: unknown[]) => runs.length ? Promise.reject(new Error("Model missing")) : Promise.resolve([]));
  useStore.setState({aiMobileClipGrouping:true});useStore.getState().regroup();
  await vi.waitFor(() => expect(useStore.getState().aiGroupingBusy).toBe(0));
  expect(useStore.getState().aiGroupingError).toContain("Model missing");
  expect(useStore.getState().poses).toEqual([]);
});

it("waits for complete metadata before constructing candidate runs", async () => {
  useStore.setState({metaProgress:0,aiMobileClipGrouping:true});
  useStore.getState().regroup();
  expect(api.aiCompare.mock.calls.filter(([runs])=>runs.length)).toHaveLength(0);
  api.aiCompare.mockImplementation((runs:unknown[])=>Promise.resolve(runs.length ? marks(true) : []));
  useStore.setState({metaProgress:1});useStore.getState().regroup();
  await vi.waitFor(()=>expect(useStore.getState().poses).toEqual([-1,-1]));
});

it("passes the selected preset and ignores an older preset reply", async () => {
  let resolve!: (value: Compared[][]) => void;
  api.aiCompare.mockImplementation((runs:unknown[],_poses:boolean,options:{mobileClipGrouping:string}) => !runs.length ? Promise.resolve([]) : options.mobileClipGrouping==="tight" ? new Promise(r=>{resolve=r;}) : Promise.resolve(marks(true)));
  useStore.setState({aiMobileClipGrouping:true,mobileClipGrouping:"tight"});useStore.getState().regroup();
  expect(api.aiCompare.mock.calls.find(([runs])=>runs.length)?.[2]).toMatchObject({mobileClipGrouping:"tight"});
  useStore.setState({mobileClipGrouping:"loose"});useStore.getState().regroup();
  await vi.waitFor(()=>expect(useStore.getState().poses).toEqual([-1,-1]));
  resolve(marks(false));await Promise.resolve();await Promise.resolve();
  expect(useStore.getState().poses).toEqual([-1,-1]);
  expect(useStore.getState().ai).toEqual([ai,ai]);
  expect(useStore.getState().aiGroupingBusy).toBe(0);
});
