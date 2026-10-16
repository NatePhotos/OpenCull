import { isVideo } from "@/lib/media";
import { useEffect, useRef, useState, type ReactNode } from "react";
import { Aperture, Calendar, Camera, Clock, FileText, Folder, HardDrive, Image as ImageIcon, Info, Ratio, Sun, Target, Timer, X } from "lucide-react";

import { api } from "@/lib/api";
import * as fmt from "@/lib/format";
import { useStore } from "@/lib/store";
import type { Histogram as HistogramData, Photo } from "@/lib/types";

/** The panel down the left: a histogram, then what the camera recorded. */
export function ExifPanel() {
  const photo = useStore((s) => (s.current >= 0 ? s.photos[s.current] : undefined));

  return (
    <div className="flex h-full min-h-0 w-full flex-col bg-panel">
      <div className="flex items-center gap-2 bg-toolbar px-3.5 py-3">
        <Info className="size-3.5 fill-brand text-toolbar" />
        <span className="flex-1 text-[13px] font-bold text-white">EXIF Data</span>
        <button type="button" title="Close Panel" onClick={() => useStore.getState().set("showExif", false)} className="rounded-full bg-white/[0.08] p-1 text-[#808080] hover:text-white">
          <X className="size-2.5" strokeWidth={3} />
        </button>
      </div>
      <div className="border-y border-[#2e2e2e]">
        {photo && isVideo(photo.path) ? <p className="p-3 text-xs text-muted-foreground">Histogram unavailable for video</p> : <Histogram photo={photo} />}
      </div>
      {photo ? <Details photo={photo} /> : <Empty />}
    </div>
  );
}

function Empty() {
  return (
    <div className="flex flex-1 flex-col items-center justify-center gap-2 text-xs text-[#666]">
      <ImageIcon className="size-7 text-[#4d4d4d]" />
      No image selected
    </div>
  );
}

function Details({ photo }: { photo: Photo }) {
  const e = photo.exif;
  const comp = fmt.exposureComp(e?.exposureComp);
  const taken = fmt.cameraDate(e?.date);
  const ext = (path: string) => path.split(".").pop()?.toUpperCase() ?? "";

  return (
    <div className="flex-1 space-y-4 overflow-y-auto p-3.5 select-text">
      <Section title="Exposure">
        <div className="grid grid-cols-2 gap-2">
          <Badge title="Shutter" value={fmt.shutter(e?.shutter)} icon={<Timer />} />
          <Badge title="Aperture" value={fmt.aperture(e?.aperture)} icon={<Aperture />} />
          <Badge title="ISO" value={e?.iso ? `ISO ${e.iso}` : null} icon={<Sun />} />
          <Badge title="Focal Length" value={fmt.focal(e?.focalLength)} icon={<Target />} />
        </div>
        {comp && comp !== "0 EV" && <div className="pt-0.5 font-mono text-[11px] text-[#cccccc]">± Exposure Comp: {comp}</div>}
      </Section>
      <Rule />
      <Section title="Equipment">
        <Row icon={<Camera />} title="Camera" value={fmt.camera(e) ?? "Unknown Camera"} />
        <Row icon={<Aperture />} title="Lens" value={e?.lens ?? "Unknown Lens"} />
      </Section>
      <Rule />
      <Section title="Date & Time">
        {taken ? (
          <>
            <Row icon={<Calendar />} title="Date" value={taken.day} />
            <Row icon={<Clock />} title="Time" value={taken.time} />
          </>
        ) : (
          <Row icon={<Calendar />} title="Date Taken" value="Not available" />
        )}
      </Section>
      <Rule />
      <Section title="File Details">
        <Row icon={<FileText />} title="Filename" value={photo.name} />
        {photo.subpath && <Row icon={<Folder />} title="Subfolder" value={photo.subpath} />}
        {e?.width && e?.height && <Row icon={<Ratio />} title="Dimensions" value={`${e.width} × ${e.height} (${fmt.megapixels(e.width, e.height)} MP)`} />}
        <Row icon={<HardDrive />} title="File Size" value={fmt.fileSize(photo.size)} />
        <Row icon={<FileText />} title="Format" value={photo.display !== photo.path ? `${ext(photo.path)} + JPEG` : ext(photo.path)} />
      </Section>
    </div>
  );
}

const Rule = () => <div className="h-px bg-[#262626]" />;

function Section({ title, children }: { title: string; children: ReactNode }) {
  return (
    <div className="space-y-2.5">
      <div className="text-[10px] font-bold uppercase text-[#737373]">{title}</div>
      {children}
    </div>
  );
}

function Badge({ title, value, icon }: { title: string; value: string | null; icon: ReactNode }) {
  return (
    <div className="rounded-md border border-white/[0.08] bg-white/[0.04] px-2.5 py-2">
      <div className="flex items-center gap-1 text-[9px] font-medium uppercase text-[#737373] [&_svg]:size-2.5 [&_svg]:text-[#808080]">
        {icon}
        {title}
      </div>
      <div className="mt-1 truncate font-mono text-[13px] font-bold text-white">{value ?? "—"}</div>
    </div>
  );
}

function Row({ icon, title, value }: { icon: ReactNode; title: string; value: string }) {
  return (
    <div className="flex items-start gap-2">
      <span className="mt-0.5 flex w-3.5 justify-center [&_svg]:size-[11px] [&_svg]:text-brand">{icon}</span>
      <div className="min-w-0">
        <div className="text-[10px] font-medium text-[#737373]">{title}</div>
        <div className="break-words font-mono text-[11px] text-[#d9d9d9]">{value}</div>
      </div>
    </div>
  );
}

const H = 96;

/**
 * How the photo's tones are spread, dark to light.
 *
 * Waits a moment after the photo changes before asking. Holding an arrow key
 * passes through dozens of photos a second, and a histogram for each would be
 * work done for pictures nobody stopped to look at.
 */
function Histogram({ photo }: { photo: Photo | undefined }) {
  const canvas = useRef<HTMLCanvasElement>(null);
  const [data, setData] = useState<HistogramData | null>(null);
  const [width, setWidth] = useState(0);
  const rgb = useStore((s) => s.rgbHistogram);
  const path = photo?.display;
  const version = photo?.version;

  useEffect(() => {
    if (!path || version === undefined) return setData(null);
    let stale = false;
    const timer = setTimeout(() => {
      api.histogram(path, version).then((h) => !stale && setData(h), () => !stale && setData(null));
    }, 120);
    return () => {
      stale = true;
      clearTimeout(timer);
    };
  }, [path, version]);

  // The canvas is sized to the panel. Dragging the panel changes that, and
  // the drawing has to follow or it stays the width the panel opened at.
  useEffect(() => {
    const el = canvas.current;
    if (!el) return;
    const measure = () => setWidth((w) => (el.clientWidth === w ? w : el.clientWidth));
    const observer = new ResizeObserver(measure);
    observer.observe(el);
    measure();
    return () => observer.disconnect();
  }, []);

  useEffect(() => {
    const el = canvas.current;
    if (!el || width === 0) return;
    const dpr = window.devicePixelRatio;
    const w = width;
    el.width = w * dpr;
    el.height = H * dpr;
    const ctx = el.getContext("2d")!;
    ctx.scale(dpr, dpr);
    ctx.clearRect(0, 0, w, H);
    if (!data) return;

    // Scaled to the tallest bin short of the very ends: a blown sky or a
    // black border piles up in the last bin and would flatten everything
    // else into a line along the bottom.
    const inner = (v: number[]) => Math.max(...v.slice(2, 254));
    const peak = rgb ? Math.max(1, inner(data.r), inner(data.g), inner(data.b), inner(data.luma)) : Math.max(1, inner(data.luma));
    const draw = (values: number[], fill: string) => {
      ctx.beginPath();
      ctx.moveTo(0, H);
      values.forEach((v, i) => ctx.lineTo((i / 255) * w, H - Math.min(1, v / peak) * (H - 4)));
      ctx.lineTo(w, H);
      ctx.closePath();
      ctx.fillStyle = fill;
      ctx.fill();
    };
    if (!rgb) {
      draw(data.luma, "rgba(255, 255, 255, .78)");
      return;
    }
    // Added together where they overlap, so equal parts of all three read as grey.
    ctx.globalCompositeOperation = "lighter";
    draw(data.r, "rgba(235, 70, 70, .62)");
    draw(data.g, "rgba(70, 200, 90, .62)");
    draw(data.b, "rgba(70, 120, 245, .62)");
    ctx.globalCompositeOperation = "source-over";
    draw(data.luma, "rgba(255, 255, 255, .16)");
  }, [data, rgb, width]);

  return <canvas ref={canvas} className="block w-full bg-[#101014]" style={{ height: H }} />;
}
