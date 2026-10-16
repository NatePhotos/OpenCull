import { create } from "zustand";
import { useEffect } from "react";
import { load, save } from "@/lib/prefs";
import "@/home-featured.css";

type HomeLayout = "spacious" | "classic";
export const useHomeLayout = create<{ layout: HomeLayout; setLayout: (layout: HomeLayout) => void }>((set) => ({
  layout: load<string>("homeLayout", "spacious") === "classic" ? "classic" : "spacious",
  setLayout(layout) { save("homeLayout", layout); set({ layout }); },
}));

export const useFeaturedImage = create<{ enabled: boolean; set: (enabled: boolean) => void }>((set) => ({
  enabled: load("showFeaturedImage", false),
  set(enabled) { save("showFeaturedImage", enabled); set({ enabled }); },
}));
export function useHomeAppearance() {
  const enabled = useFeaturedImage(s => s.enabled);
  useEffect(() => { document.documentElement.dataset.homeFeatured = String(enabled); }, [enabled]);
}
export function HomeLayoutSettings() {
  const { layout, setLayout } = useHomeLayout();
  const featured = useFeaturedImage();
  return <fieldset className="space-y-3 rounded-md border border-border">
    <legend className="px-1">Home layout</legend>
    <label className="flex items-center gap-2"><input type="checkbox" checked={featured.enabled} onChange={e => featured.set(e.target.checked)} />Show a featured image on home page</label>
    <div className="flex flex-col gap-3">
      <label className="flex items-center gap-2"><input type="radio" name="home-layout" checked={layout === "spacious"} onChange={() => setLayout("spacious")} />Spacious · actions below heading</label>
      <label className="flex items-center gap-2"><input type="radio" name="home-layout" checked={layout === "classic"} onChange={() => setLayout("classic")} />Classic · actions at upper right</label>
    </div>
    <p className="text-sm text-muted-foreground">Controls wrap on smaller windows. Home layout is independent of theme, font and scale.</p>
  </fieldset>;
}
