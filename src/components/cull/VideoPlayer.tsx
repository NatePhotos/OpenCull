import { useEffect, useState } from "react";
import { convertFileSrc, invoke } from "@tauri-apps/api/core";
import { inTauri } from "@/lib/api";

export function VideoPlayer({ path }: { path: string }) {
  const [url, setUrl] = useState("");
  const [error, setError] = useState("");
  useEffect(() => {
    let stale = false; setUrl(""); setError("");
    if (!inTauri) { setError("Video playback is available in the desktop application."); return; }
    void invoke<string>("video_source", { path }).then((file) => { if (!stale) setUrl(file.startsWith("http://127.0.0.1:") ? file : convertFileSrc(file)); }, (e) => { if (!stale) setError(String(e)); });
    return () => { stale = true; };
  }, [path]);
  return <div data-video-player className="flex size-full flex-col items-center justify-center gap-3 px-4 pb-20 pt-4" onKeyDown={(e) => e.stopPropagation()}>
    {url && !error && <video key={url} src={url} controls playsInline preload="metadata" aria-label="Video playback" className="min-h-0 max-h-full w-full flex-1 object-contain"
      onError={() => setError("This video could not be played by the system player. You can still keep, reject, or import the file. On Linux, playback uses the installed WebKitGTK/GStreamer codecs.")} />}
    {error && <p role="status" className="max-w-lg text-center text-sm text-foreground">{error}</p>}
    {!url && !error && <p className="text-sm text-muted-foreground">Opening video…</p>}
  </div>;
}
