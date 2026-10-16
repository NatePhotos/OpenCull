import { beforeEach, describe, expect, it } from "vitest";

import { holdPreferences, load, save } from "./prefs";

describe("a self-test leaves the settings as they were", () => {
  beforeEach(() => localStorage.clear());

  it("saves a setting until it is told to hold, and nothing after", () => {
    save("recents", ["/photos/one"]);
    expect(load<string[]>("recents", [])).toEqual(["/photos/one"]);

    holdPreferences();
    save("recents", ["/scratch/selftest"]);
    save("position./scratch/selftest", "/scratch/selftest/1.ARW");
    save("smartGroups", false);

    expect(load<string[]>("recents", [])).toEqual(["/photos/one"]);
    expect(localStorage.getItem("opencull.position./scratch/selftest")).toBeNull();
    expect(localStorage.getItem("opencull.smartGroups")).toBeNull();
  });
});
