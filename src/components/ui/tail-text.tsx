import { useLayoutEffect, useRef, useState } from "react";

/**
 * The end of `text` that fits, with "…" in front when something was cut:
 * a folder path is known by its last part, so that is the part that stays.
 * `fits` says whether a candidate is narrow enough.
 */
export function tailThatFits(text: string, fits: (candidate: string) => boolean): string {
  if (fits(text)) return text;
  // The most that fits is found by halving: the longest end for which "…" + end still fits.
  let low = 1;
  let high = text.length;
  while (low < high) {
    const mid = (low + high) >> 1;
    if (fits(`…${text.slice(mid)}`)) high = mid;
    else low = mid + 1;
  }
  return `…${text.slice(low)}`;
}

/**
 * One line of text that never grows past its box. If it does not fit, it shows
 * its end, led by "…". Measured rather than left to the browser, so it reads
 * the same in every web view; the whole text is its tooltip.
 */
export function TailText({ text, className }: { text: string; className?: string }) {
  const node = useRef<HTMLSpanElement>(null);
  const [shown, setShown] = useState(text);

  useLayoutEffect(() => {
    const el = node.current;
    if (!el) return;
    const canvas = document.createElement("canvas").getContext("2d");
    const fit = () => {
      if (!canvas) return setShown(text);
      const style = getComputedStyle(el);
      canvas.font = `${style.fontStyle} ${style.fontWeight} ${style.fontSize} ${style.fontFamily}`;
      const room = el.clientWidth - parseFloat(style.paddingLeft) - parseFloat(style.paddingRight);
      setShown(room > 0 ? tailThatFits(text, (candidate) => canvas.measureText(candidate).width <= room) : text);
    };
    fit();
    const observer = new ResizeObserver(fit);
    observer.observe(el);
    return () => observer.disconnect();
  }, [text]);

  return (
    <span ref={node} className={className} style={{ display: "block", minWidth: 0, contain: "inline-size", overflow: "hidden", whiteSpace: "nowrap" }} title={text}>
      {shown}
    </span>
  );
}
