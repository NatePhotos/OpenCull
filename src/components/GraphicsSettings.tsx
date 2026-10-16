import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { inTauri } from "@/lib/api";

export function GraphicsSettings() {
  const [result, setResult] = useState("");
  const [busy, setBusy] = useState(false);
  const check = async () => {
    setBusy(true);
    try {
      const info = inTauri ? await invoke<{requested:string;effective?:string;platform:string;dmabufDisabled:boolean;softwareForced:boolean;sharedMemoryTransport:boolean;fallbackCompositor:boolean}>("graphics_info") : null;
      const canvas = document.createElement("canvas");
      const gl = canvas.getContext("webgl");
      let renderer = "WebGL unavailable";
      if (gl) {
        const debug = gl.getExtension("WEBGL_debug_renderer_info");
        renderer = String(gl.getParameter(debug ? debug.UNMASKED_RENDERER_WEBGL : gl.RENDERER));
        gl.getExtension("WEBGL_lose_context")?.loseContext();
      }
      const maskedName = info?.platform === "linux" && /apple gpu/i.test(renderer)
        ? "\nWebKit returned a generic Apple GPU name on Linux. This does not identify the physical GPU."
        : "";
      setResult(`${info ? `${info.platform} · requested profile: ${info.requested} · active profile: ${info.effective ?? info.requested}${info.sharedMemoryTransport ? " · shared-memory transport" : ""}${info.dmabufDisabled ? " · DMA-BUF disabled" : ""}${info.softwareForced ? " · software forced" : ""}${info.fallbackCompositor ? " · fallback compositor" : ""}` : "Browser renderer"}\nWebGL-reported name (may be masked): ${renderer}${maskedName}`);
    } catch(e) { setResult(`Renderer check failed: ${String(e)}`); }
    finally { setBusy(false); }
  };
  return <fieldset className="space-y-3 rounded-lg border border-border p-4">
    <legend className="px-1 text-sm font-semibold">Graphics acceleration</legend>
    <p className="text-xs leading-relaxed text-muted-foreground">The desktop webview renders the interface and photo transformations. This check shows the requested and active launch profiles and the webview’s reported device name, which may be masked. Neither confirms the physical GPU in use; use native system tools to verify it. Image decoding and AI can still use the CPU.</p>
    <button type="button" disabled={busy} onClick={() => void check()} className="min-h-10 rounded-md border border-input px-3 text-xs">{busy ? "Checking…" : "Check renderer"}</button>
    {result && <output aria-live="polite" className="block whitespace-pre-wrap break-words text-xs leading-relaxed">{result}</output>}
  </fieldset>;
}
