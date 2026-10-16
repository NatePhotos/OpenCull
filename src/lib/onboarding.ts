/** Existing installations keep their normal launch; only an empty profile is new. */
export function shouldShowOverview(values: Record<string, unknown>): boolean {
  return values.onboardingSeen !== true && Object.keys(values).every((key) => key === "onboardingSeen");
}
