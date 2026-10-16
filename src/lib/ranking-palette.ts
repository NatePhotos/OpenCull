export type RankingPalette = "classic" | "red-green";
export function validRankingPalette(value: unknown): RankingPalette {
  return value === "red-green" ? value : "classic";
}

/** Rank one is best. Keep numeric labels so colour is never the only cue. */
export function rankingColour(rank: number, count: number, palette: RankingPalette): string {
  if (!Number.isFinite(rank) || !Number.isFinite(count) || rank < 1 || count < 1) return "#525252";
  if (palette === "classic") {
    const third = Math.max(1, Math.ceil(count / 3));
    return rank === 1 ? "#2ea0fa" : rank <= third ? "#8cbde6" : rank > count - third ? "#de8a8a" : "#525252";
  }
  const position = Math.max(0, Math.min(1, (rank - 1) / Math.max(1, count - 1)));
  const green = [51, 204, 102], yellow = [230, 199, 106], red = [222, 138, 138];
  const [from, to, share] = position <= .5 ? [green, yellow, position * 2] : [yellow, red, (position - .5) * 2];
  return `#${from.map((value, index) => Math.round(value + (to[index] - value) * share).toString(16).padStart(2, "0")).join("")}`;
}
