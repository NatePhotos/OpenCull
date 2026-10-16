import { describe, expect, it } from "vitest";
import { latestSession, type RecentSession } from "./session-history";
const card = (id: string, updatedAt: number): RecentSession => ({id,updatedAt,name:id,sources:["/card"],count:1,stage:"review",cover:null});
describe("resume latest session", () => {
  it("chooses the most recently used session regardless of kind or list order", () => {
    expect(latestSession([card("a",1),card("b",3)],{path:"/folder",updatedAt:2})).toEqual({kind:"card",id:"b"});
    expect(latestSession([card("a",1)],{path:"/folder",updatedAt:2})).toEqual({kind:"folder",path:"/folder"});
  });
  it("supports existing recent folders and ignores malformed preferences", () => {
    expect(latestSession([],null,"/old-folder")).toEqual({kind:"folder",path:"/old-folder"});
    expect(latestSession([], {path:"/bad",updatedAt:"wrong"})).toBeNull();
    expect(latestSession([],null)).toBeNull();
  });
});
