import { describe, expect, it } from "vitest";
import { shouldShowOverview } from "./onboarding";
describe("first launch overview", () => {
  it("opens for an empty profile and stays dismissed once seen", () => {
    expect(shouldShowOverview({})).toBe(true);
    expect(shouldShowOverview({ onboardingSeen: true })).toBe(false);
  });
  it("does not interrupt an existing user's launch", () => {
    expect(shouldShowOverview({ theme: "grey" })).toBe(false);
    expect(shouldShowOverview({ recents: [] })).toBe(false);
  });
});
