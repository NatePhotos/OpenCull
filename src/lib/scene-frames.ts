/** The contiguous scene containing the current frame, following the visible order. */
export function sceneFrames(view: readonly number[], current: number, groupOf: (index: number) => number | null): number[] {
  const at = view.indexOf(current);
  if (at < 0) return [];
  const group = groupOf(current);
  if (group === null) return [current];
  let start = at, end = at + 1;
  while (start > 0 && groupOf(view[start - 1]) === group) start--;
  while (end < view.length && groupOf(view[end]) === group) end++;
  return view.slice(start, end);
}
