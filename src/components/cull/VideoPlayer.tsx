import { useEffect, useRef, useState } from "react";
import { convertFileSrc, invoke } from "@tauri-apps/api/core";
import { inTauri } from "@/lib/api";
import { videoThumb } from "@/lib/video-thumb";

export function VideoPlayer({ path }: { path: string }) {
  const [url, setUrl] = useState("");
  const [error, setError] = useState("");
  const player = useRef<HTMLVideoElement>(null);
  const [poster, setPoster] = useState<string>();
  // The first frame, large, so a clip waiting to be played is not a black box.
  useEffect(() => {
    let live = true;
    setPoster(undefined);
    void videoThumb(path, 1280, () => live).then((url) => live && url && setPoster(url));
    return () => { live = false; };
  }, [path]);
  // Space plays and pauses the video in view, ahead of whatever Space does for a photo.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.code !== "Space" || e.metaKey || e.ctrlKey || e.altKey || e.shiftKey) return;
      const target = e.target as HTMLElement | null;
      if (target && (target.isContentEditable || /^(INPUT|TEXTAREA|SELECT|BUTTON)$/.test(target.tagName))) return;
      if (document.querySelector('[role="dialog"], [role="alertdialog"], [role="menu"]')) return;
      const video = player.current;
      if (!video) return;
      e.preventDefault();
      e.stopImmediatePropagation();
      if (e.repeat) return;
      if (video.paused) void video.play().catch(() => {}); else video.pause();
    };
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  }, []);
  useEffect(() => {
    let stale = false; setUrl(""); setError("");
    if (!inTauri) { setError("Video playback is available in the desktop application."); return; }
    void invoke<string>("video_source", { path }).then((file) => { if (!stale) setUrl(file.startsWith("http://127.0.0.1:") ? file : convertFileSrc(file)); }, (e) => { if (!stale) setError(String(e)); });
    return () => { stale = true; };
  }, [path]);
  return <div data-video-player className="flex size-full flex-col items-center justify-center gap-3 px-4 pb-20 pt-4">
    {url && !error && <video ref={player} key={url} src={url} poster={poster} controls onFocus={(e) => e.currentTarget.blur()} playsInline preload="metadata" aria-label="Video playback" className="min-h-0 max-h-full w-full flex-1 object-contain"
      onError={() => setError("This video could not be played by the system player. You can still keep, reject, or import the file. On Linux, playback uses the installed WebKitGTK/GStreamer codecs.")} />}
    {error && <p role="status" className="max-w-lg text-center text-sm text-foreground">{error}</p>}
    {!url && !error && <p className="text-sm text-muted-foreground">Opening video…</p>}
  </div>;
}
